use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post},
};
use axum_server::tls_rustls::RustlsConfig;
use chrono::{DateTime, SecondsFormat, Utc};
use chrono_tz::Asia::Shanghai;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use singer_core::{KaraokeCatalog, KaraokeRecording, KaraokeSong};
use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    path::{Path as FsPath, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::Mutex,
};

const MAX_CHUNK_BYTES: usize = 16 << 20;
const BUILD_COMMIT: &str = match option_env!("SINGER_BUILD_COMMIT") {
    Some(v) => v,
    None => "dev",
};
static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct AppState {
    recordings: Arc<RecordingStore>,
    karaoke: Arc<KaraokeService>,
    karaoke_recordings: Arc<KaraokeRecordingStore>,
    data_dir: PathBuf,
    web_dir: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
struct SessionView {
    id: String,
    started_at: String,
    seq: u64,
    mime: String,
    bytes: u64,
}

#[derive(Clone, Debug)]
struct Session {
    id: String,
    started_at: DateTime<Utc>,
    seq: u64,
    mime: String,
    bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
struct Recording {
    id: String,
    file: String,
    bytes: u64,
    modified: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<Value>,
}

struct RecordingStore {
    dir: PathBuf,
    sessions: Mutex<HashMap<String, Session>>,
}

impl RecordingStore {
    async fn new(dir: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&dir).await?;
        Ok(Self {
            dir,
            sessions: Mutex::new(HashMap::new()),
        })
    }

    async fn start(&self, mime: String) -> Result<SessionView, String> {
        let id = new_id();
        let started_at = Utc::now();
        let part = self.dir.join(format!("{id}.part"));
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&part)
            .await
            .map_err(err_text)?;
        let session = Session {
            id: id.clone(),
            started_at,
            seq: 0,
            mime: mime.clone(),
            bytes: 0,
        };
        self.sessions.lock().await.insert(id.clone(), session);
        Ok(SessionView {
            id,
            started_at: started_at.to_rfc3339_opts(SecondsFormat::Nanos, true),
            seq: 0,
            mime,
            bytes: 0,
        })
    }

    async fn append(&self, id: &str, seq: u64, chunk: &[u8]) -> Result<usize, String> {
        if !safe_id(id) {
            return Err("invalid recording id".into());
        }
        if chunk.len() > MAX_CHUNK_BYTES {
            return Err("chunk exceeds 16 MiB".into());
        }
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| "recording session not active".to_string())?;
        if seq != session.seq {
            return Err(format!("sequence mismatch: expected {}", session.seq));
        }
        let mut file = OpenOptions::new()
            .append(true)
            .open(self.dir.join(format!("{id}.part")))
            .await
            .map_err(err_text)?;
        file.write_all(chunk).await.map_err(err_text)?;
        session.seq += 1;
        session.bytes += chunk.len() as u64;
        Ok(chunk.len())
    }

    async fn finalize(&self, id: &str, client: Value) -> Result<Recording, String> {
        if !safe_id(id) {
            return Err("invalid recording id".into());
        }
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get(id)
            .cloned()
            .ok_or_else(|| "recording session not active".to_string())?;
        let file = format!("{}{}", id, ext_for_mime(&session.mime));
        fs::rename(self.dir.join(format!("{id}.part")), self.dir.join(&file))
            .await
            .map_err(err_text)?;
        let ended = Utc::now();
        let meta = json!({
            "id": id,
            "started_at": session.started_at.to_rfc3339_opts(SecondsFormat::Nanos, true),
            "ended_at": ended.to_rfc3339_opts(SecondsFormat::Nanos, true),
            "mime": session.mime,
            "bytes": session.bytes,
            "chunks": session.seq,
            "file": file,
            "client": client,
        });
        let pretty = serde_json::to_vec_pretty(&meta).map_err(err_text)?;
        fs::write(self.dir.join(format!("{id}.json")), pretty)
            .await
            .map_err(err_text)?;
        sessions.remove(id);
        Ok(Recording {
            id: id.to_string(),
            file,
            bytes: session.bytes,
            modified: ended.to_rfc3339_opts(SecondsFormat::Nanos, true),
            meta: Some(meta),
        })
    }

    async fn list(&self) -> Vec<Recording> {
        let mut out = Vec::new();
        let Ok(mut entries) = fs::read_dir(&self.dir).await else {
            return out;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(ft) = entry.file_type().await else {
                continue;
            };
            if ft.is_dir() || name.ends_with(".part") || name.ends_with(".json") {
                continue;
            }
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            let id = name
                .rsplit_once('.')
                .map(|x| x.0)
                .unwrap_or(&name)
                .to_string();
            let modified = meta
                .modified()
                .ok()
                .map(system_time_rfc3339)
                .unwrap_or_default();
            let sidecar = fs::read(self.dir.join(format!("{id}.json")))
                .await
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok());
            out.push(Recording {
                id,
                file: name,
                bytes: meta.len(),
                modified,
                meta: sidecar,
            });
        }
        out.sort_by(|a, b| b.modified.cmp(&a.modified));
        out
    }

    async fn audio_path(&self, id: &str) -> Option<PathBuf> {
        if !safe_id(id) {
            return None;
        }
        let mut entries = fs::read_dir(&self.dir).await.ok()?;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&format!("{id}."))
                && !name.ends_with(".json")
                && !name.ends_with(".part")
            {
                return Some(entry.path());
            }
        }
        None
    }

    async fn delete(&self, id: &str) {
        if !safe_id(id) {
            return;
        }
        self.sessions.lock().await.remove(id);
        if let Ok(mut entries) = fs::read_dir(&self.dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(&format!("{id}.")) {
                    let _ = fs::remove_file(entry.path()).await;
                }
            }
        }
    }
}

struct KaraokeService {
    root: PathBuf,
    assets: PathBuf,
    catalog: KaraokeCatalog,
}

impl KaraokeService {
    async fn load(root: PathBuf) -> Self {
        let assets = root.join("assets");
        let _ = fs::create_dir_all(&assets).await;
        let mut catalog: KaraokeCatalog = fs::read(root.join("catalog.json"))
            .await
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .filter(|c: &KaraokeCatalog| c.provider == "manual-curated-local")
            .unwrap_or_else(|| KaraokeCatalog {
                provider: "manual-curated-local".into(),
                language: "粵語/國語".into(),
                ..Default::default()
            });
        catalog.language = "粵語/國語".into();
        let mut valid = Vec::new();
        for mut song in catalog.songs.drain(..) {
            if Self::validate_song(&assets, &mut song).await {
                valid.push(song);
            }
        }
        catalog.songs = valid;
        Self {
            root,
            assets,
            catalog,
        }
    }

    async fn validate_song(assets: &FsPath, song: &mut KaraokeSong) -> bool {
        if song.id.is_empty()
            || song.title.is_empty()
            || song.artist.is_empty()
            || !matches!(song.language.as_str(), "粵語" | "國語")
        {
            return false;
        }
        let mut count = 0;
        for mode in ["original", "accompaniment"] {
            let Some(track) = song.tracks.get_mut(mode) else {
                continue;
            };
            if track.mode != mode
                || track.source_type != "local"
                || track.source_file.is_empty()
                || !matches!(track.language.as_str(), "粵語" | "國語")
            {
                return false;
            }
            let Some(ext) = FsPath::new(&track.source_file)
                .extension()
                .and_then(|x| x.to_str())
            else {
                return false;
            };
            if ext.len() > 7 {
                return false;
            }
            let path = assets
                .join(&song.id)
                .join(format!("{mode}.{}", ext.to_ascii_lowercase()));
            if fs::metadata(&path)
                .await
                .map(|m| m.is_file() && m.len() > 0)
                .unwrap_or(false)
            {
                track.url = format!("/singeros/api/karaoke/assets/{}/{mode}", song.id);
                count += 1;
            } else {
                return false;
            }
        }
        count > 0
    }

    fn asset_path(&self, song_id: &str, mode: &str) -> Option<PathBuf> {
        let song = self.catalog.songs.iter().find(|s| s.id == song_id)?;
        let track = song.tracks.get(mode)?;
        let ext = FsPath::new(&track.source_file)
            .extension()?
            .to_str()?
            .to_ascii_lowercase();
        let path = self.assets.join(song_id).join(format!("{mode}.{ext}"));
        path.is_file().then_some(path)
    }
}

#[derive(Clone, Debug)]
struct KaraokeSession {
    id: String,
    song_id: String,
    song_title: String,
    mode: String,
    mime: String,
    started: DateTime<Utc>,
    seq: u64,
    bytes: u64,
}

struct KaraokeRecordingStore {
    dir: PathBuf,
    sessions: Mutex<HashMap<String, KaraokeSession>>,
}

impl KaraokeRecordingStore {
    async fn new(dir: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&dir).await?;
        Ok(Self {
            dir,
            sessions: Mutex::new(HashMap::new()),
        })
    }

    async fn start(
        &self,
        song_id: String,
        song_title: String,
        mode: String,
        mime: String,
    ) -> Result<(String, String), String> {
        let id = new_id();
        let started = Utc::now();
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(self.dir.join(format!("{id}.part")))
            .await
            .map_err(err_text)?;
        self.sessions.lock().await.insert(
            id.clone(),
            KaraokeSession {
                id: id.clone(),
                song_id,
                song_title,
                mode,
                mime,
                started,
                seq: 0,
                bytes: 0,
            },
        );
        Ok((id, shanghai_time(started)))
    }

    async fn append(&self, id: &str, seq: u64, chunk: &[u8]) -> Result<usize, String> {
        if !safe_id(id) {
            return Err("invalid recording id".into());
        }
        if chunk.len() > MAX_CHUNK_BYTES {
            return Err("chunk exceeds 16 MiB".into());
        }
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| "recording session not active".to_string())?;
        if session.seq != seq {
            return Err(format!("sequence mismatch: expected {}", session.seq));
        }
        let mut file = OpenOptions::new()
            .append(true)
            .open(self.dir.join(format!("{id}.part")))
            .await
            .map_err(err_text)?;
        file.write_all(chunk).await.map_err(err_text)?;
        session.seq += 1;
        session.bytes += chunk.len() as u64;
        Ok(chunk.len())
    }

    async fn finalize(&self, id: &str, meta: Value) -> Result<KaraokeRecording, String> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get(id)
            .cloned()
            .ok_or_else(|| "recording session not active".to_string())?;
        let now = Utc::now();
        let supplied = meta
            .get("duration_seconds")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .round() as i64;
        let duration = if (1..=21600).contains(&supplied) {
            supplied
        } else {
            (now - session.started).num_milliseconds().max(0) / 1000
        };
        let ext = ext_for_mime(&session.mime);
        let stamp = now.with_timezone(&Shanghai).format("%Y%m%d_%H%M%S");
        let base = format!("{}_{}", safe_song_filename(&session.song_title), stamp);
        let mut file = format!("{base}{ext}");
        if fs::metadata(self.dir.join(&file)).await.is_ok() {
            file = format!(
                "{}_{}{}",
                base,
                session.id.chars().take(8).collect::<String>(),
                ext
            );
        }
        fs::rename(self.dir.join(format!("{id}.part")), self.dir.join(&file))
            .await
            .map_err(err_text)?;
        let rec = KaraokeRecording {
            id: id.into(),
            file,
            song_id: session.song_id,
            song_title: session.song_title,
            mode: session.mode,
            duration_seconds: duration,
            created_at_shanghai: shanghai_time(now),
            started_at_shanghai: shanghai_time(session.started),
            bytes: session.bytes,
            url: format!("/singeros/api/karaoke/recordings/{id}/audio"),
        };
        let pretty = serde_json::to_vec_pretty(&rec).map_err(err_text)?;
        fs::write(self.dir.join(format!("{id}.json")), pretty)
            .await
            .map_err(err_text)?;
        sessions.remove(id);
        Ok(rec)
    }

    async fn list(&self) -> Vec<KaraokeRecording> {
        let mut out = Vec::new();
        let Ok(mut entries) = fs::read_dir(&self.dir).await else {
            return out;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".json") {
                continue;
            }
            if let Ok(bytes) = fs::read(entry.path()).await {
                if let Ok(rec) = serde_json::from_slice::<KaraokeRecording>(&bytes) {
                    if !rec.id.is_empty() {
                        out.push(rec);
                    }
                }
            }
        }
        out.sort_by(|a, b| b.created_at_shanghai.cmp(&a.created_at_shanghai));
        out
    }

    async fn audio_path(&self, id: &str) -> Option<PathBuf> {
        if !safe_id(id) {
            return None;
        }
        let bytes = fs::read(self.dir.join(format!("{id}.json"))).await.ok()?;
        let rec: KaraokeRecording = serde_json::from_slice(&bytes).ok()?;
        if rec.file.is_empty() {
            return None;
        }
        let path = self.dir.join(FsPath::new(&rec.file).file_name()?);
        fs::metadata(&path).await.ok()?.is_file().then_some(path)
    }
}

#[derive(Deserialize)]
struct StartRecording {
    #[serde(default)]
    mime: String,
}
#[derive(Deserialize)]
struct SeqQuery {
    seq: i64,
}
#[derive(Deserialize)]
struct StartKaraoke {
    song_id: String,
    song_title: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    mime: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let data_dir = PathBuf::from(
        env::var("SINGER_DATA").unwrap_or_else(|_| "/opt/singeros/recordings".into()),
    );
    let karaoke_root = PathBuf::from(
        env::var("SINGER_KARAOKE").unwrap_or_else(|_| "/opt/singeros/karaoke".into()),
    );
    let web_dir = PathBuf::from(
        env::var("SINGER_WEB").unwrap_or_else(|_| "/opt/singeros-rs/current/web".into()),
    );
    let cert = PathBuf::from(
        env::var("SINGER_CERT")
            .unwrap_or_else(|_| "/etc/letsencrypt/live/www4399.sbs/fullchain.pem".into()),
    );
    let key = PathBuf::from(
        env::var("SINGER_KEY")
            .unwrap_or_else(|_| "/etc/letsencrypt/live/www4399.sbs/privkey.pem".into()),
    );
    let addr: SocketAddr = env::var("SINGER_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:18443".into())
        .parse()
        .expect("valid SINGER_ADDR");

    let recordings = Arc::new(
        RecordingStore::new(data_dir.clone())
            .await
            .expect("recording directory"),
    );
    let karaoke = Arc::new(KaraokeService::load(karaoke_root).await);
    let karaoke_recordings = Arc::new(
        KaraokeRecordingStore::new(data_dir.join("karaoke"))
            .await
            .expect("karaoke recording directory"),
    );
    let state = AppState {
        recordings,
        karaoke,
        karaoke_recordings,
        data_dir,
        web_dir,
    };

    let app = Router::new()
        .route("/", get(root_redirect))
        .route("/singeros", get(singer_redirect))
        .route("/singeros/", get(index))
        .route("/singeros/karaoke", get(karaoke_redirect))
        .route("/singeros/karaoke/", get(index))
        .route("/singeros/pkg/{*path}", get(pkg_asset))
        .route("/singeros/healthz", get(health))
        .route("/singeros/api/capabilities", get(capabilities))
        .route("/singeros/api/client-log", post(client_log))
        .route("/singeros/api/recordings/start", post(recording_start))
        .route("/singeros/api/recordings/{id}/chunk", post(recording_chunk))
        .route(
            "/singeros/api/recordings/{id}/finalize",
            post(recording_finalize),
        )
        .route("/singeros/api/recordings", get(recording_list))
        .route("/singeros/api/recordings/{id}/audio", get(recording_audio))
        .route("/singeros/api/recordings/{id}", delete(recording_delete))
        .route("/singeros/api/karaoke/catalog", get(karaoke_catalog))
        .route(
            "/singeros/api/karaoke/assets/{song}/{mode}",
            get(karaoke_asset),
        )
        .route(
            "/singeros/api/karaoke/recordings/start",
            post(karaoke_recording_start),
        )
        .route(
            "/singeros/api/karaoke/recordings/{id}/chunk",
            post(karaoke_recording_chunk),
        )
        .route(
            "/singeros/api/karaoke/recordings/{id}/finalize",
            post(karaoke_recording_finalize),
        )
        .route(
            "/singeros/api/karaoke/recordings",
            get(karaoke_recording_list),
        )
        .route(
            "/singeros/api/karaoke/recordings/{id}/audio",
            get(karaoke_recording_audio),
        )
        .layer(DefaultBodyLimit::max(MAX_CHUNK_BYTES + 1024))
        .layer(middleware::from_fn(security_headers))
        .with_state(state);

    let tls = RustlsConfig::from_pem_file(cert, key)
        .await
        .expect("load TLS material");
    eprintln!("SingerOS-rs {BUILD_COMMIT} listening on https://{addr}");
    axum_server::bind_rustls(addr, tls)
        .serve(app.into_make_service())
        .await
        .expect("serve SingerOS-rs");
}

async fn root_redirect() -> Redirect {
    Redirect::temporary("/singeros/")
}
async fn singer_redirect() -> Redirect {
    Redirect::temporary("/singeros/")
}
async fn karaoke_redirect() -> Redirect {
    Redirect::temporary("/singeros/karaoke/")
}

async fn index(State(state): State<AppState>) -> Response {
    file_response(&state.web_dir.join("index.html"), None).await
}

async fn pkg_asset(
    State(state): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !safe_relative(&path) {
        return StatusCode::NOT_FOUND.into_response();
    }
    file_response(&state.web_dir.join("pkg").join(path), Some(&headers)).await
}

async fn health() -> Json<Value> {
    Json(json!({"ok":true,"app":"SingerOS-rs","commit":BUILD_COMMIT}))
}
async fn capabilities() -> Json<Value> {
    Json(
        json!({"handwritten_business_js":0,"server":"rust","browser":"rust-wasm","tls":"rustls","nginx":false}),
    )
}

async fn client_log(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    if body.len() > 64 << 10 {
        return api_error(StatusCode::PAYLOAD_TOO_LARGE, "log exceeds 64 KiB");
    }
    let client: Value = serde_json::from_slice(&body)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&body).into_owned()));
    let entry = json!({
        "server_time": Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true),
        "user_agent": headers.get(header::USER_AGENT).and_then(|x| x.to_str().ok()).unwrap_or(""),
        "client": client,
    });
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(state.data_dir.join("client.log"))
        .await
    {
        if let Ok(mut line) = serde_json::to_vec(&entry) {
            line.push(b'\n');
            let _ = file.write_all(&line).await;
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn recording_start(
    State(state): State<AppState>,
    Json(req): Json<StartRecording>,
) -> Response {
    match state.recordings.start(req.mime).await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn recording_chunk(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<SeqQuery>,
    body: Bytes,
) -> Response {
    if q.seq < 0 {
        return api_error(StatusCode::BAD_REQUEST, "invalid seq");
    }
    match state.recordings.append(&id, q.seq as u64, &body).await {
        Ok(n) => Json(json!({"seq":q.seq,"bytes":n})).into_response(),
        Err(e) => api_error(StatusCode::CONFLICT, e),
    }
}

async fn recording_finalize(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(meta): Json<Value>,
) -> Response {
    match state.recordings.finalize(&id, meta).await {
        Ok(rec) => {
            Json(json!({"url":format!("/singeros/api/recordings/{}/audio",rec.id),"recording":rec}))
                .into_response()
        }
        Err(e) => api_error(StatusCode::CONFLICT, e),
    }
}
async fn recording_list(State(state): State<AppState>) -> Json<Vec<Recording>> {
    Json(state.recordings.list().await)
}
async fn recording_audio(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    match state.recordings.audio_path(&id).await {
        Some(path) => file_response(&path, Some(&headers)).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn recording_delete(State(state): State<AppState>, Path(id): Path<String>) -> StatusCode {
    state.recordings.delete(&id).await;
    StatusCode::NO_CONTENT
}

async fn karaoke_catalog(State(state): State<AppState>) -> Json<KaraokeCatalog> {
    Json(state.karaoke.catalog.clone())
}
async fn karaoke_asset(
    State(state): State<AppState>,
    Path((song, mode)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    match state.karaoke.asset_path(&song, &mode) {
        Some(path) => file_response(&path, Some(&headers)).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn karaoke_recording_start(
    State(state): State<AppState>,
    Json(req): Json<StartKaraoke>,
) -> Response {
    if req.song_id.is_empty() || req.song_title.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "song_id and song_title required");
    }
    match state
        .karaoke_recordings
        .start(req.song_id, req.song_title, req.mode, req.mime)
        .await
    {
        Ok((id, started)) => (
            StatusCode::CREATED,
            Json(json!({"id":id,"started_at_shanghai":started})),
        )
            .into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}
async fn karaoke_recording_chunk(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<SeqQuery>,
    body: Bytes,
) -> Response {
    if q.seq < 0 {
        return api_error(StatusCode::BAD_REQUEST, "invalid seq");
    }
    match state
        .karaoke_recordings
        .append(&id, q.seq as u64, &body)
        .await
    {
        Ok(n) => Json(json!({"seq":q.seq,"bytes":n})).into_response(),
        Err(e) => api_error(StatusCode::CONFLICT, e),
    }
}
async fn karaoke_recording_finalize(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(meta): Json<Value>,
) -> Response {
    match state.karaoke_recordings.finalize(&id, meta).await {
        Ok(rec) => Json(rec).into_response(),
        Err(e) => api_error(StatusCode::CONFLICT, e),
    }
}
async fn karaoke_recording_list(State(state): State<AppState>) -> Json<Vec<KaraokeRecording>> {
    Json(state.karaoke_recordings.list().await)
}
async fn karaoke_recording_audio(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    match state.karaoke_recordings.audio_path(&id).await {
        Some(path) => file_response(&path, Some(&headers)).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn security_headers(req: Request<Body>, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    for (name, value) in [
        (
            header::CACHE_CONTROL,
            "no-store, no-cache, must-revalidate, max-age=0",
        ),
        (header::PRAGMA, "no-cache"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("microphone=(self), camera=()"),
    );
    headers.insert(
        HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        HeaderName::from_static("cross-origin-embedder-policy"),
        HeaderValue::from_static("require-corp"),
    );
    response
}

async fn file_response(path: &FsPath, request_headers: Option<&HeaderMap>) -> Response {
    let Ok(meta) = fs::metadata(path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !meta.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let total = meta.len();
    let range_header = request_headers
        .and_then(|h| h.get(header::RANGE))
        .and_then(|x| x.to_str().ok());
    let range = match range_header {
        Some(v) => match parse_range(v, total) {
            Some(v) => Some(v),
            None => {
                let mut r = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
                r.headers_mut().insert(
                    header::CONTENT_RANGE,
                    HeaderValue::from_str(&format!("bytes */{total}")).unwrap(),
                );
                return r;
            }
        },
        None => None,
    };
    let (start, end, status) = range
        .map(|(s, e)| (s, e, StatusCode::PARTIAL_CONTENT))
        .unwrap_or((0, total.saturating_sub(1), StatusCode::OK));
    let count = if total == 0 {
        0
    } else {
        end.saturating_sub(start) + 1
    };
    let mut file = match File::open(path).await {
        Ok(v) => v,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if start > 0 && file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let mut bytes = Vec::with_capacity(count.min(32 << 20) as usize);
    if count > 0 && file.take(count).read_to_end(&mut bytes).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let mut response = (status, Body::from(bytes)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(path)),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&count.to_string()).unwrap(),
    );
    if status == StatusCode::PARTIAL_CONTENT {
        headers.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {start}-{end}/{total}")).unwrap(),
        );
    }
    response
}

fn parse_range(value: &str, total: u64) -> Option<(u64, u64)> {
    let spec = value.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (left, right) = spec.split_once('-')?;
    if total == 0 {
        return None;
    }
    if left.is_empty() {
        let suffix: u64 = right.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        let start = total.saturating_sub(suffix.min(total));
        return Some((start, total - 1));
    }
    let start: u64 = left.parse().ok()?;
    if start >= total {
        return None;
    }
    let end = if right.is_empty() {
        total - 1
    } else {
        right.parse::<u64>().ok()?.min(total - 1)
    };
    (end >= start).then_some((start, end))
}

fn content_type(path: &FsPath) -> &'static str {
    match path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "ogg" | "opus" => "audio/ogg",
        "webm" => "audio/webm",
        "m4a" | "mp4" => "audio/mp4",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        _ => "application/octet-stream",
    }
}

fn api_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({"error":message.into()}))).into_response()
}
fn err_text<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}
fn safe_id(id: &str) -> bool {
    (8..=96).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains('\\'))
}
fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{:x}-{counter:x}", std::process::id())
}
fn ext_for_mime(mime: &str) -> &'static str {
    match mime.split(';').next().unwrap_or("").trim() {
        "audio/ogg" => ".ogg",
        "audio/mp4" => ".m4a",
        "audio/webm" => ".webm",
        _ => ".bin",
    }
}
fn system_time_rfc3339(t: SystemTime) -> String {
    DateTime::<Utc>::from(t).to_rfc3339_opts(SecondsFormat::Nanos, true)
}
fn shanghai_time(t: DateTime<Utc>) -> String {
    t.with_timezone(&Shanghai)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}
fn safe_song_filename(input: &str) -> String {
    let mut out = String::new();
    for ch in input.trim().chars() {
        if out.chars().count() >= 80 {
            break;
        }
        if ch.is_control() || matches!(ch, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    let clean = out.trim_matches(|c| c == '.' || c == ' ').to_string();
    if clean.is_empty() {
        "karaoke".into()
    } else {
        clean
    }
}
