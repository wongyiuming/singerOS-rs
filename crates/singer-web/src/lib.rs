#[cfg(target_arch = "wasm32")]
mod wasm_app {
    use gloo_net::http::Request;
    use js_sys::Uint8Array;
    use leptos::prelude::*;
    use serde::Deserialize;
    use serde_json::{Value, json};
    use std::{cell::RefCell, collections::HashMap};
    use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::*};
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{Blob, BlobEvent, MediaRecorder, MediaStream, MediaStreamConstraints};

    thread_local! {
        static MIC_STREAM: RefCell<Option<MediaStream>> = const { RefCell::new(None) };
        static RECORDER: RefCell<Option<MediaRecorder>> = const { RefCell::new(None) };
        static CHUNKS: RefCell<Vec<Blob>> = const { RefCell::new(Vec::new()) };
    }

    #[derive(Clone, Debug, Deserialize)]
    struct Catalog {
        #[serde(default)]
        songs: Vec<Song>,
    }

    #[derive(Clone, Debug, Deserialize)]
    struct Song {
        id: String,
        title: String,
        artist: String,
        #[serde(default)]
        album: String,
        #[serde(default)]
        language: String,
        #[serde(default)]
        tracks: HashMap<String, Value>,
    }

    fn set_status(status: RwSignal<String>, value: impl Into<String>) {
        status.set(value.into());
    }

    async fn load_catalog(status: RwSignal<String>, songs: RwSignal<Vec<Song>>) {
        match Request::get("/singeros/api/karaoke/catalog").send().await {
            Ok(response) if response.ok() => match response.json::<Catalog>().await {
                Ok(catalog) => {
                    let count = catalog.songs.len();
                    songs.set(catalog.songs);
                    set_status(status, format!("曲库已连接 · {count} 首"));
                }
                Err(err) => set_status(status, format!("曲库解析失败：{err}")),
            },
            Ok(response) => set_status(status, format!("曲库请求失败：HTTP {}", response.status())),
            Err(err) => set_status(status, format!("曲库连接失败：{err}")),
        }
    }

    async fn request_microphone(status: RwSignal<String>, mic_ready: RwSignal<bool>) {
        set_status(status, "正在请求麦克风权限…");
        let Some(window) = web_sys::window() else {
            set_status(status, "浏览器 Window 不可用");
            return;
        };
        let media = match window.navigator().media_devices() {
            Ok(media) => media,
            Err(err) => {
                set_status(status, format!("MediaDevices 不可用：{err:?}"));
                return;
            }
        };
        let constraints = MediaStreamConstraints::new();
        constraints.set_audio(&JsValue::TRUE);
        constraints.set_video(&JsValue::FALSE);
        let promise = match media.get_user_media_with_constraints(&constraints) {
            Ok(promise) => promise,
            Err(err) => {
                set_status(status, format!("无法发起麦克风请求：{err:?}"));
                return;
            }
        };
        match JsFuture::from(promise).await {
            Ok(value) => {
                let stream = MediaStream::from(value);
                MIC_STREAM.with(|slot| *slot.borrow_mut() = Some(stream));
                mic_ready.set(true);
                set_status(status, "麦克风已就绪 · Rust/WASM 控制");
            }
            Err(err) => set_status(status, format!("麦克风权限失败：{err:?}")),
        }
    }

    fn start_recording(status: RwSignal<String>, recording: RwSignal<bool>) {
        let stream = MIC_STREAM.with(|slot| slot.borrow().clone());
        let Some(stream) = stream else {
            set_status(status, "请先授权麦克风");
            return;
        };
        let recorder = match MediaRecorder::new_with_media_stream(&stream) {
            Ok(recorder) => recorder,
            Err(err) => {
                set_status(status, format!("MediaRecorder 初始化失败：{err:?}"));
                return;
            }
        };
        CHUNKS.with(|chunks| chunks.borrow_mut().clear());

        let data_cb = Closure::<dyn FnMut(BlobEvent)>::new(move |event: BlobEvent| {
            if let Some(blob) = event.data() {
                if blob.size() > 0.0 {
                    CHUNKS.with(|chunks| chunks.borrow_mut().push(blob));
                }
            }
        });
        recorder.set_ondataavailable(Some(data_cb.as_ref().unchecked_ref()));
        data_cb.forget();

        let mime = recorder.mime_type();
        let stop_status = status;
        let stop_recording = recording;
        let stop_cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let mime = if mime.is_empty() {
                "audio/webm".to_string()
            } else {
                mime.clone()
            };
            spawn_local(async move {
                upload_recording(mime, stop_status).await;
                stop_recording.set(false);
            });
        });
        recorder.set_onstop(Some(stop_cb.as_ref().unchecked_ref()));
        stop_cb.forget();

        match recorder.start_with_time_slice(1000) {
            Ok(()) => {
                RECORDER.with(|slot| *slot.borrow_mut() = Some(recorder));
                recording.set(true);
                set_status(status, "录音中 · 数据仅在停止后上传");
            }
            Err(err) => set_status(status, format!("录音启动失败：{err:?}")),
        }
    }

    fn stop_recording(status: RwSignal<String>) {
        let recorder = RECORDER.with(|slot| slot.borrow().clone());
        match recorder {
            Some(recorder) => {
                set_status(status, "正在封装并上传录音…");
                if let Err(err) = recorder.stop() {
                    set_status(status, format!("停止录音失败：{err:?}"));
                }
                RECORDER.with(|slot| *slot.borrow_mut() = None);
            }
            None => set_status(status, "当前没有活动录音"),
        }
    }

    async fn upload_recording(mime: String, status: RwSignal<String>) {
        let start_body = json!({"mime": mime}).to_string();
        let start_request = match Request::post("/singeros/api/recordings/start")
            .header("Content-Type", "application/json")
            .body(start_body)
        {
            Ok(request) => request,
            Err(err) => {
                set_status(status, format!("创建录音请求失败：{err}"));
                return;
            }
        };
        let response = match start_request.send().await {
            Ok(response) if response.ok() => response,
            Ok(response) => {
                set_status(status, format!("创建录音失败：HTTP {}", response.status()));
                return;
            }
            Err(err) => {
                set_status(status, format!("创建录音失败：{err}"));
                return;
            }
        };
        let payload: Value = match response.json().await {
            Ok(payload) => payload,
            Err(err) => {
                set_status(status, format!("录音会话响应解析失败：{err}"));
                return;
            }
        };
        let Some(id) = payload.get("id").and_then(Value::as_str).map(str::to_owned) else {
            set_status(status, "录音会话缺少 id");
            return;
        };

        let blobs = CHUNKS.with(|chunks| std::mem::take(&mut *chunks.borrow_mut()));
        for (seq, blob) in blobs.into_iter().enumerate() {
            let array_buffer = match JsFuture::from(blob.array_buffer()).await {
                Ok(value) => value,
                Err(err) => {
                    set_status(status, format!("录音分片读取失败：{err:?}"));
                    return;
                }
            };
            let array = Uint8Array::new(&array_buffer);
            let mut bytes = vec![0; array.length() as usize];
            array.copy_to(&mut bytes);
            let url = format!("/singeros/api/recordings/{id}/chunk?seq={seq}");
            let request = match Request::post(&url).body(bytes) {
                Ok(request) => request,
                Err(err) => {
                    set_status(status, format!("录音分片请求失败：{err}"));
                    return;
                }
            };
            match request.send().await {
                Ok(response) if response.ok() => {}
                Ok(response) => {
                    set_status(
                        status,
                        format!("录音分片 {seq} 上传失败：HTTP {}", response.status()),
                    );
                    return;
                }
                Err(err) => {
                    set_status(status, format!("录音分片 {seq} 上传失败：{err}"));
                    return;
                }
            }
        }

        let finalize = format!("/singeros/api/recordings/{id}/finalize");
        let request = match Request::post(&finalize)
            .header("Content-Type", "application/json")
            .body("{}")
        {
            Ok(request) => request,
            Err(err) => {
                set_status(status, format!("录音完成请求失败：{err}"));
                return;
            }
        };
        match request.send().await {
            Ok(response) if response.ok() => {
                set_status(status, format!("录音已保存 · {id}"));
            }
            Ok(response) => set_status(status, format!("录音完成失败：HTTP {}", response.status())),
            Err(err) => set_status(status, format!("录音完成失败：{err}")),
        }
    }

    #[component]
    fn App() -> impl IntoView {
        let status = RwSignal::new("正在连接 singerOS Rust 服务…".to_string());
        let mic_ready = RwSignal::new(false);
        let recording = RwSignal::new(false);
        let songs = RwSignal::new(Vec::<Song>::new());
        let selected_song = RwSignal::new(None::<String>);
        let song_gain = RwSignal::new(35_u16);
        let record_gain = RwSignal::new(170_u16);

        spawn_local(load_catalog(status, songs));

        view! {
            <style>{r#"
                :root{--bg:#090b10;--panel:#121722;--line:#273043;--text:#f5f7fb;--muted:#8e98aa;--accent:#7c5cff;--good:#65dda0;--bad:#ff7c86}
                *{box-sizing:border-box} body{margin:0;background:radial-gradient(900px 560px at 12% -10%,#27204b 0,transparent 62%),var(--bg);color:var(--text)}
                button,input{font:inherit}.shell{max-width:1380px;margin:auto;padding:28px}.top{display:flex;justify-content:space-between;align-items:center;gap:18px;margin-bottom:20px}
                .brand h1{margin:0;font-size:28px}.brand p{margin:5px 0 0;color:var(--muted)}.badge{padding:9px 13px;border:1px solid var(--line);border-radius:999px;color:var(--good);background:#111621}
                .grid{display:grid;grid-template-columns:1.25fr .75fr;gap:18px}.panel{border:1px solid var(--line);border-radius:22px;background:rgba(18,23,34,.88);padding:22px;box-shadow:0 24px 80px #0006}
                h2,h3{margin:0 0 14px}.controls{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:14px}.control{border:1px solid var(--line);border-radius:16px;padding:16px;background:#0d1119}
                .control label{display:flex;justify-content:space-between;color:#cbd2df;margin-bottom:12px}.control input{width:100%}.actions{display:flex;gap:10px;flex-wrap:wrap;margin-top:16px}
                button{cursor:pointer;border:1px solid var(--line);border-radius:14px;padding:12px 16px;background:#171d2a;color:var(--text);font-weight:700}button.primary{background:linear-gradient(135deg,#7c5cff,#6544e9)}button.danger{background:#351921;border-color:#6e2b38}button:disabled{opacity:.4;cursor:not-allowed}
                .status{margin-top:16px;padding:13px;border-radius:14px;background:#0d1119;color:#b9c2d1;border:1px solid var(--line)}.songs{display:flex;flex-direction:column;gap:8px;max-height:520px;overflow:auto}.song{width:100%;text-align:left;background:#0d1119}.song.active{border-color:#7c5cff;background:#18132b}.song strong{display:block}.song span{color:var(--muted);font-size:12px}.empty{color:var(--muted);padding:24px 0}
                .foot{margin-top:18px;color:var(--muted);font-size:12px}@media(max-width:850px){.grid{grid-template-columns:1fr}.shell{padding:16px}.controls{grid-template-columns:1fr}.top{align-items:flex-start;flex-direction:column}}
            "#}</style>
            <main class="shell">
                <header class="top">
                    <div class="brand">
                        <h1>"singerOS"</h1>
                        <p>"Rust/WASM vocal console · RN production runtime"</p>
                    </div>
                    <div class="badge">{move || if recording.get() { "● RECORDING" } else if mic_ready.get() { "● MIC READY" } else { "● RUST ONLINE" }}</div>
                </header>
                <section class="grid">
                    <div class="panel">
                        <h2>"演唱控制台"</h2>
                        <div class="controls">
                            <div class="control">
                                <label><span>"伴奏音量"</span><b>{move || format!("{}%", song_gain.get())}</b></label>
                                <input type="range" min="0" max="100"
                                    prop:value=move || song_gain.get()
                                    on:input=move |ev| song_gain.set(event_target_value(&ev).parse().unwrap_or(35)) />
                            </div>
                            <div class="control">
                                <label><span>"人声录制增益"</span><b>{move || format!("{}%", record_gain.get())}</b></label>
                                <input type="range" min="0" max="300"
                                    prop:value=move || record_gain.get()
                                    on:input=move |ev| record_gain.set(event_target_value(&ev).parse().unwrap_or(170)) />
                            </div>
                        </div>
                        <div class="actions">
                            <button class="primary" disabled=move || mic_ready.get()
                                on:click=move |_| spawn_local(request_microphone(status, mic_ready))>
                                "授权麦克风"
                            </button>
                            <button disabled=move || !mic_ready.get() || recording.get()
                                on:click=move |_| start_recording(status, recording)>
                                "开始录音"
                            </button>
                            <button class="danger" disabled=move || !recording.get()
                                on:click=move |_| stop_recording(status)>
                                "停止并保存"
                            </button>
                        </div>
                        <div class="status">{move || status.get()}</div>
                        <div class="foot">"录音分片、曲库状态与持久化均通过 Rust 服务契约完成；页面业务状态运行在 WASM 中。"</div>
                    </div>
                    <div class="panel">
                        <h3>"曲库"</h3>
                        <div class="songs">
                            <Show when=move || !songs.get().is_empty() fallback=|| view! { <div class="empty">"当前曲库为空或尚未同步。"</div> }>
                                <For
                                    each=move || songs.get()
                                    key=|song| song.id.clone()
                                    children=move |song: Song| {
                                        let id = song.id.clone();
                                        let active_id = id.clone();
                                        let select_id = id.clone();
                                        let track_count = song.tracks.len();
                                        view! {
                                            <button class:active=move || selected_song.get().as_deref() == Some(active_id.as_str())
                                                class="song"
                                                on:click=move |_| selected_song.set(Some(select_id.clone()))>
                                                <strong>{song.title}</strong>
                                                <span>{format!("{} · {} · {} · {} track(s)", song.artist, song.album, song.language, track_count)}</span>
                                            </button>
                                        }
                                    }
                                />
                            </Show>
                        </div>
                    </div>
                </section>
            </main>
        }
    }

    #[wasm_bindgen(start)]
    pub fn start() {
        mount_to_body(App);
    }
}
