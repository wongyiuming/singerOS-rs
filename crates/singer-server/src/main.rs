use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Path as AxumPath, Query, State},
    http::{HeaderValue, Request, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{delete, get, post},
};
use chrono::{DateTime, Utc};
use chrono_tz::Asia::Shanghai;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

const APP_NAME: &str = "singerOS-rs";
const MAX_CHUNK_BYTES: u64 = 16 << 20;
const INDEX_HTML: &str = include_str!("index.html");

fn build_commit() -> &'static str {
    option_env!("SINGER_BUILD_COMMIT").unwrap_or("dev")
}

#[derive(Clone)]
struct AppState {
    recordings: Arc<RecordingStore>,
    karaoke: Arc<KaraokeService>,
    karaoke_recordings: Arc<KaraokeRecordingStore>,
    data_dir: Arc<PathBuf>,
    web_root: Arc<PathBuf>,
}

#[derive(Debug, Serialize, Clone)]
struct SessionView {
    id: String,
    started_at: DateTime<Utc>,
    seq: u64,
    mime: String,
    bytes: u64,
}

#[derive(Debug, Clone)]
struct Session {
    view: SessionView,
}

#[derive(Debug, Serialize, Clone)]
struct Recording {
    id: String,
    file: String,
    bytes: u64,
    modified: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<Value>,
}

struct RecordingStore {
    dir: PathBuf,
    sessions: Mutex<HashMap<String, Session>>,
}

impl RecordingStore {
    fn new(dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            sessions: Mutex::new(HashMap::new()),
        })
    }

    fn start(&self, mime: String) -> io::Result<SessionView> {
        let id = new_id();
        let view = SessionView {
            id: id.clone(),
            started_at: Utc::now(),
            seq: 0,
            mime,
            bytes: 0,
        };
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(self.dir.join(format!("{id}.part")))?;
        self.sessions
            .lock()
            .expect("recording lock")
            .insert(id, Session { view: view.clone() });
        Ok(view)
    }

    fn append(&self, id: &str, seq: u64, body: &[u8]) -> Result<u64, String> {
        if !safe_id(id) {
            return Err("invalid recording id".into());
        }
        if body.len() as u64 > MAX_CHUNK_BYTES {
            return Err("chunk exceeds 16 MiB".into());
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "recording lock poisoned")?;
        let session = sessions.get_mut(id).ok_or("recording session not active")?;
        if seq != session.view.seq {
            return Err(format!("sequence mismatch: expected {}", session.view.seq));
        }
        let mut file = OpenOptions::new()
            .append(true)
            .open(self.dir.join(format!("{id}.part")))
            .map_err(|e| e.to_string())?;
        file.write_all(body).map_err(|e| e.to_string())?;
        session.view.seq += 1;
        session.view.bytes += body.len() as u64;
        Ok(body.len() as u64)
    }

    fn finalize(&self, id: &str, client_meta: Value) -> Result<Recording, String> {
        if !safe_id(id) {
            return Err("invalid recording id".into());
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "recording lock poisoned")?;
        let session = sessions.remove(id).ok_or("recording session not active")?;
        let ext = ext_for_mime(&session.view.mime);
        let final_name = format!("{id}{ext}");
        fs::rename(
            self.dir.join(format!("{id}.part")),
            self.dir.join(&final_name),
        )
        .map_err(|e| e.to_string())?;
        let meta = json!({
            "id": id,
            "started_at": session.view.started_at,
            "ended_at": Utc::now(),
            "mime": session.view.mime,
            "bytes": session.view.bytes,
            "chunks": session.view.seq,
            "file": final_name,
            "client": client_meta,
        });
        fs::write(
            self.dir.join(format!("{id}.json")),
            serde_json::to_vec_pretty(&meta).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(Recording {
            id: id.into(),
            file: final_name,
            bytes: session.view.bytes,
            modified: Utc::now(),
            meta: Some(meta),
        })
    }

    fn list(&self) -> Vec<Recording> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".part") || name.ends_with(".json") || name == "client.log" {
                continue;
            }
            let id = Path::new(&name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            if !safe_id(&id) {
                continue;
            }
            let modified = meta
                .modified()
                .ok()
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(Utc::now);
            let sidecar = fs::read(self.dir.join(format!("{id}.json")))
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

    fn audio_path(&self, id: &str) -> Option<PathBuf> {
        if !safe_id(id) {
            return None;
        }
        fs::read_dir(&self.dir)?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.is_file()
                    && p.file_stem().and_then(|s| s.to_str()) == Some(id)
                    && p.extension().and_then(|s| s.to_str()) != Some("json")
                    && p.extension().and_then(|s| s.to_str()) != Some("part")
            })
    }

    fn delete(&self, id: &str) {
        if !safe_id(id) {
            return;
        }
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(id);
        }
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.file_stem().and_then(|s| s.to_str()) == Some(id) {
                    let _ = fs::remove_file(p);
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct KaraokeCue {
    start: f64,
    end: f64,
    text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct KaraokeTrack {
    mode: String,
    version: String,
    source_type: String,
    source_file: String,
    #[serde(default)]
    source_page: String,
    #[serde(default)]
    license: String,
    language: String,
    #[serde(default)]
    lyrics_language: String,
    #[serde(default)]
    lyrics_offset_seconds: f64,
    duration_seconds: f64,
    bytes: u64,
    url: String,
    #[serde(default)]
    lyrics: Vec<KaraokeCue>,
    #[serde(default)]
    synced_at_shanghai: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct KaraokeSong {
    id: String,
    title: String,
    artist: String,
    #[serde(default)]
    album: String,
    #[serde(default)]
    year: i32,
    #[serde(default)]
    track_no: i32,
    version: String,
    language: String,
    #[serde(default)]
    lyrics_language: String,
    #[serde(default)]
    lyrics_version: String,
    #[serde(default)]
    lyrics: Vec<KaraokeCue>,
    #[serde(default)]
    tracks: HashMap<String, KaraokeTrack>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct KaraokeAlbum {
    id: String,
    title: String,
    year: i32,
    language: String,
    order: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct KaraokeCatalog {
    provider: String,
    language: String,
    #[serde(default)]
    updated_at_shanghai: String,
    #[serde(default)]
    albums: Vec<KaraokeAlbum>,
    #[serde(default)]
    songs: Vec<KaraokeSong>,
}

struct KaraokeService {
    assets: PathBuf,
    catalog: RwLock<KaraokeCatalog>,
}

impl KaraokeService {
    fn new(data_dir: &Path) -> io::Result<Self> {
        let root = data_dir.parent().unwrap_or(data_dir).join("karaoke");
        let assets = root.join("assets");
        fs::create_dir_all(&assets)?;
        let catalog = fs::read(root.join("catalog.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<KaraokeCatalog>(&b).ok())
            .unwrap_or_else(|| KaraokeCatalog {
                provider: "manual-curated-local".into(),
                language: "粵語/國語".into(),
                ..Default::default()
            });
        Ok(Self {
            assets,
            catalog: RwLock::new(catalog),
        })
    }

    fn snapshot(&self) -> KaraokeCatalog {
        self.catalog.read().expect("catalog lock").clone()
    }

    fn asset(&self, song_id: &str, mode: &str) -> Option<PathBuf> {
        if !safe_component(song_id) || !matches!(mode, "original" | "accompaniment") {
            return None;
        }
        let catalog = self.catalog.read().ok()?;
        let song = catalog.songs.iter().find(|s| s.id == song_id)?;
        let track = song.tracks.get(mode)?;
        if track.source_type != "local" {
            return None;
        }
        let ext = Path::new(track.source_file.trim_start_matches("File:"))
            .extension()?
            .to_str()?;
        if ext.len() > 8 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        let p = self.assets.join(song_id).join(format!("{mode}.{ext}"));
        p.is_file().then_some(p)
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
struct KaraokeRecording {
    id: String,
    file: String,
    song_id: String,
    song_title: String,
    mode: String,
    duration_seconds: i64,
    created_at_shanghai: String,
    started_at_shanghai: String,
    bytes: u64,
    url: String,
}

struct KaraokeRecordingStore {
    dir: PathBuf,
    sessions: Mutex<HashMap<String, KaraokeSession>>,
}

impl KaraokeRecordingStore {
    fn new(data_dir: &Path) -> io::Result<Self> {
        let dir = data_dir.join("karaoke");
        fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            sessions: Mutex::new(HashMap::new()),
        })
    }

    fn start(
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
            .map_err(|e| e.to_string())?;
        self.sessions
            .lock()
            .map_err(|_| "karaoke lock poisoned")?
            .insert(
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
        Ok((
            id,
            started
                .with_timezone(&Shanghai)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        ))
    }

    fn append(&self, id: &str, seq: u64, body: &[u8]) -> Result<u64, String> {
        if !safe_id(id) {
            return Err("invalid recording id".into());
        }
        if body.len() as u64 > MAX_CHUNK_BYTES {
            return Err("chunk exceeds 16 MiB".into());
        }
        let mut sessions = self.sessions.lock().map_err(|_| "karaoke lock poisoned")?;
        let s = sessions.get_mut(id).ok_or("recording session not active")?;
        if s.seq != seq {
            return Err(format!("sequence mismatch: expected {}", s.seq));
        }
        OpenOptions::new()
            .append(true)
            .open(self.dir.join(format!("{id}.part")))
            .and_then(|mut f| f.write_all(body))
            .map_err(|e| e.to_string())?;
        s.seq += 1;
        s.bytes += body.len() as u64;
        Ok(body.len() as u64)
    }

    fn finalize(&self, id: &str, meta: Value) -> Result<KaraokeRecording, String> {
        let mut sessions = self.sessions.lock().map_err(|_| "karaoke lock poisoned")?;
        let s = sessions.remove(id).ok_or("recording session not active")?;
        let created = Utc::now();
        let mut duration = meta
            .get("duration_seconds")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .round() as i64;
        if duration <= 0 || duration > 6 * 3600 {
            duration = (created - s.started).num_seconds().max(0);
        }
        let base = format!(
            "{}_{}",
            safe_song_filename(&s.song_title),
            created.with_timezone(&Shanghai).format("%Y%m%d_%H%M%S")
        );
        let ext = ext_for_mime(&s.mime);
        let mut final_name = format!("{base}{ext}");
        if self.dir.join(&final_name).exists() {
            final_name = format!("{base}_{}{ext}", &id[..id.len().min(8)]);
        }
        fs::rename(
            self.dir.join(format!("{id}.part")),
            self.dir.join(&final_name),
        )
        .map_err(|e| e.to_string())?;
        let rec = KaraokeRecording {
            id: s.id,
            file: final_name,
            song_id: s.song_id,
            song_title: s.song_title,
            mode: s.mode,
            duration_seconds: duration,
            created_at_shanghai: created
                .with_timezone(&Shanghai)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            started_at_shanghai: s
                .started
                .with_timezone(&Shanghai)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            bytes: s.bytes,
            url: format!("/singeros/api/karaoke/recordings/{id}/audio"),
        };
        fs::write(
            self.dir.join(format!("{id}.json")),
            serde_json::to_vec_pretty(&rec).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(rec)
    }

    fn list(&self) -> Vec<KaraokeRecording> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(b) = fs::read(&p) {
                    if let Ok(r) = serde_json::from_slice(&b) {
                        out.push(r);
                    }
                }
            }
        }
        out.sort_by(|a, b| b.created_at_shanghai.cmp(&a.created_at_shanghai));
        out
    }

    fn audio_path(&self, id: &str) -> Option<PathBuf> {
        if !safe_id(id) {
            return None;
        }
        let rec: KaraokeRecording =
            serde_json::from_slice(&fs::read(self.dir.join(format!("{id}.json"))).ok()?).ok()?;
        let p = self.dir.join(rec.file);
        p.is_file().then_some(p)
    }
}

#[derive(Deserialize)]
struct StartRecording {
    #[serde(default)]
    mime: String,
}

#[derive(Deserialize)]
struct StartKaraokeRecording {
    song_id: String,
    song_title: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    mime: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let addr: SocketAddr = std::env::var("SINGER_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()
        .expect("valid SINGER_ADDR");
    let data_dir = PathBuf::from(
        std::env::var("SINGER_DATA").unwrap_or_else(|_| "/opt/singeros/data/recordings".into()),
    );
    let web_root = PathBuf::from(
        std::env::var("SINGER_WEB_ROOT").unwrap_or_else(|_| "/opt/singeros/web".into()),
    );
    fs::create_dir_all(&data_dir).expect("create data directory");
    let state = AppState {
        recordings: Arc::new(RecordingStore::new(data_dir.clone()).expect("recording store")),
        karaoke: Arc::new(KaraokeService::new(&data_dir).expect("karaoke service")),
        karaoke_recordings: Arc::new(
            KaraokeRecordingStore::new(&data_dir).expect("karaoke recording store"),
        ),
        data_dir: Arc::new(data_dir),
        web_root: Arc::new(web_root),
    };

    let app = Router::new()
        .route(
            "/singeros",
            get(|| async { Redirect::temporary("/singeros/") }),
        )
        .route("/singeros/", get(index))
        .route(
            "/singeros/karaoke",
            get(|| async { Redirect::temporary("/singeros/karaoke/") }),
        )
        .route("/singeros/karaoke/", get(index))
        .route("/singeros/app/{*path}", get(web_asset))
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
        .layer(middleware::from_fn(secure_headers))
        .with_state(state);

    if let (Ok(cert), Ok(key)) = (std::env::var("SINGER_CERT"), std::env::var("SINGER_KEY")) {
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key)
            .await
            .expect("load TLS material");
        eprintln!("{APP_NAME} HTTPS {addr} commit={}", build_commit());
        axum_server::bind_rustls(addr, tls)
            .serve(app.into_make_service())
            .await
            .expect("serve SingerOS");
    } else {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("bind SingerOS");
        eprintln!(
            "{APP_NAME} HTTP {addr} commit={} (set SINGER_CERT/SINGER_KEY for direct TLS)",
            build_commit()
        );
        axum::serve(listener, app).await.expect("serve SingerOS");
    }
}

async fn secure_headers(req: Request<Body>, next: Next) -> Response {
    let mut response = next.run(req).await;
    let h = response.headers_mut();
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
    );
    h.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    h.insert(
        "permissions-policy",
        HeaderValue::from_static("microphone=(self), camera=()"),
    );
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    h.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    response
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn health() -> Json<Value> {
    Json(json!({
        "ok": true,
        "app": APP_NAME,
        "commit": build_commit(),
        "runtime": "rust"
    }))
}

async fn capabilities() -> Json<Value> {
    Json(json!({
        "handwritten_business_js": 0,
        "runtime": "rust",
        "ui": "wasm",
        "tls": "native-or-staged"
    }))
}

async fn web_asset(State(state): State<AppState>, AxumPath(path): AxumPath<String>) -> Response {
    if !safe_relative_path(&path) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    serve_file(state.web_root.join(path)).await
}

async fn client_log(State(state): State<AppState>, body: Bytes) -> StatusCode {
    if body.len() > 64 << 10 {
        return StatusCode::PAYLOAD_TOO_LARGE;
    }
    let client: Value = serde_json::from_slice(&body)
        .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&body)}));
    let line = json!({
        "server_time": Utc::now().to_rfc3339(),
        "client": client
    })
    .to_string();
    let path = state.data_dir.join("client.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
    StatusCode::NO_CONTENT
}

async fn recording_start(
    State(state): State<AppState>,
    Json(req): Json<StartRecording>,
) -> Response {
    match state.recordings.start(req.mime) {
        Ok(s) => (StatusCode::CREATED, Json(s)).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn recording_chunk(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let Some(seq) = q.get("seq").and_then(|v| v.parse::<u64>().ok()) else {
        return error(StatusCode::BAD_REQUEST, "invalid seq");
    };
    match state.recordings.append(&id, seq, &body) {
        Ok(n) => Json(json!({"seq": seq, "bytes": n})).into_response(),
        Err(e) => error(StatusCode::CONFLICT, e),
    }
}

async fn recording_finalize(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> Response {
    let meta = parse_json_object(&body);
    match state.recordings.finalize(&id, meta) {
        Ok(r) => Json(json!({
            "url": format!("/singeros/api/recordings/{}/audio", r.id),
            "recording": r
        }))
        .into_response(),
        Err(e) => error(StatusCode::CONFLICT, e),
    }
}

async fn recording_list(State(state): State<AppState>) -> Json<Vec<Recording>> {
    Json(state.recordings.list())
}

async fn recording_audio(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    match state.recordings.audio_path(&id) {
        Some(p) => serve_file(p).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn recording_delete(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> StatusCode {
    state.recordings.delete(&id);
    StatusCode::NO_CONTENT
}

async fn karaoke_catalog(State(state): State<AppState>) -> Json<KaraokeCatalog> {
    Json(state.karaoke.snapshot())
}

async fn karaoke_asset(
    State(state): State<AppState>,
    AxumPath((song, mode)): AxumPath<(String, String)>,
) -> Response {
    match state.karaoke.asset(&song, &mode) {
        Some(p) => serve_file(p).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn karaoke_recording_start(
    State(state): State<AppState>,
    Json(req): Json<StartKaraokeRecording>,
) -> Response {
    if req.song_id.trim().is_empty() || req.song_title.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "song_id and song_title required");
    }
    match state
        .karaoke_recordings
        .start(req.song_id, req.song_title, req.mode, req.mime)
    {
        Ok((id, started)) => (
            StatusCode::CREATED,
            Json(json!({"id": id, "started_at_shanghai": started})),
        )
            .into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn karaoke_recording_chunk(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let Some(seq) = q.get("seq").and_then(|v| v.parse::<u64>().ok()) else {
        return error(StatusCode::BAD_REQUEST, "invalid seq");
    };
    match state.karaoke_recordings.append(&id, seq, &body) {
        Ok(n) => Json(json!({"seq": seq, "bytes": n})).into_response(),
        Err(e) => error(StatusCode::CONFLICT, e),
    }
}

async fn karaoke_recording_finalize(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> Response {
    match state
        .karaoke_recordings
        .finalize(&id, parse_json_object(&body))
    {
        Ok(r) => Json(r).into_response(),
        Err(e) => error(StatusCode::CONFLICT, e),
    }
}

async fn karaoke_recording_list(State(state): State<AppState>) -> Json<Vec<KaraokeRecording>> {
    Json(state.karaoke_recordings.list())
}

async fn karaoke_recording_audio(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    match state.karaoke_recordings.audio_path(&id) {
        Some(p) => serve_file(p).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn serve_file(path: PathBuf) -> Response {
    let Ok(bytes) = tokio::fs::read(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = content_type(&path);
    ([(header::CONTENT_TYPE, mime)], bytes).into_response()
}

fn parse_json_object(body: &[u8]) -> Value {
    if body.is_empty() {
        return Value::Object(Map::new());
    }
    serde_json::from_slice(body).unwrap_or_else(|_| Value::Object(Map::new()))
}

fn error(status: StatusCode, message: impl ToString) -> Response {
    (status, Json(json!({"error": message.to_string()}))).into_response()
}

fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}-{:x}", std::process::id())
}

fn safe_id(v: &str) -> bool {
    (8..=96).contains(&v.len())
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn safe_component(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 128
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        && !v.contains("..")
}

fn safe_relative_path(v: &str) -> bool {
    !v.is_empty()
        && !v.starts_with('/')
        && !v.split('/').any(|p| p.is_empty() || p == "." || p == "..")
}

fn safe_song_filename(v: &str) -> String {
    let mut out = String::with_capacity(v.len().min(80));
    for c in v.trim().chars().take(80) {
        if "\\/:*?\"<>|".contains(c) || c.is_control() {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    let out = out.trim_matches(|c| c == '.' || c == ' ').to_string();
    if out.is_empty() {
        "karaoke".into()
    } else {
        out
    }
}

fn ext_for_mime(mime: &str) -> &'static str {
    match mime.split(';').next().unwrap_or_default().trim() {
        "audio/webm" => ".webm",
        "audio/ogg" => ".ogg",
        "audio/mp4" => ".m4a",
        _ => ".bin",
    }
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "webm" => "audio/webm",
        "ogg" | "opus" => "audio/ogg",
        "m4a" | "mp4" => "audio/mp4",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}
