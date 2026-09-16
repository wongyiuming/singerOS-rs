mod api;
mod audio;

use audio::{DeviceOption, RecordingSpec};
use gloo_timers::callback::Interval;
use leptos::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use singer_core::{KaraokeCatalog, KaraokeCue, KaraokeRecording, KaraokeSong};
use std::rc::Rc;
use wasm_bindgen::{closure::Closure, JsCast};
use wasm_bindgen_futures::spawn_local;
use web_sys::{HtmlAudioElement, KeyboardEvent};

const STYLE: &str = r#"
:root{--bg:#07090d;--panel:#11151dcc;--line:#ffffff18;--text:#f7f8fb;--muted:#929aac;--accent:#7c5cff;--accent2:#30d5c8;--good:#70e3a1;--bad:#ff7b86;--warn:#ffc66d}
*{box-sizing:border-box}html{background:var(--bg)}body{margin:0;min-height:100vh;color:var(--text);font-family:Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;background:radial-gradient(900px 540px at 8% -10%,#7c5cff2e,transparent 63%),radial-gradient(700px 500px at 96% 2%,#30d5c81f,transparent 62%),#07090d}
button,input,select{font:inherit}.shell{max-width:1400px;margin:auto;padding:24px}.top{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-bottom:20px}.brand{font-size:24px;font-weight:800}.sub{color:var(--muted);font-size:13px}.grid{display:grid;grid-template-columns:minmax(0,1.55fr) minmax(320px,.8fr);gap:18px}.panel{background:var(--panel);border:1px solid var(--line);border-radius:22px;padding:18px;box-shadow:0 24px 70px #0005;backdrop-filter:blur(18px)}h1,h2,h3,p{margin-top:0}h2{font-size:17px;margin-bottom:14px}.row{display:flex;align-items:center;gap:10px;flex-wrap:wrap}.stack{display:grid;gap:12px}.control{display:grid;grid-template-columns:150px 1fr 64px;gap:12px;align-items:center}.control label{color:#c9ced9;font-size:13px}.control output{text-align:right;color:var(--accent2);font-variant-numeric:tabular-nums}.control input[type=range]{width:100%;accent-color:var(--accent)}button,select{background:#171c27;color:var(--text);border:1px solid var(--line);border-radius:12px;padding:10px 13px}button{cursor:pointer}button:hover{border-color:#ffffff35}button.primary{background:linear-gradient(135deg,#7658ff,#5c45d8);border-color:#9a87ff66}button.danger{background:#441b24;border-color:#ff7b8655}button.active{outline:2px solid #7c5cff88}button:disabled{opacity:.45;cursor:not-allowed}select{min-width:220px;max-width:100%}.meter{height:10px;border-radius:999px;background:#ffffff10;overflow:hidden}.meter>i{display:block;height:100%;background:linear-gradient(90deg,var(--accent2),var(--warn),var(--bad));transition:width .08s linear}.status{padding:10px 12px;border-radius:12px;background:#ffffff09;color:#cfd4df;font-size:13px}.route{font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-size:12px;color:var(--good);padding:10px;background:#07120d;border:1px solid #70e3a12e;border-radius:12px}.albums{display:grid;gap:16px}.album{border-top:1px solid var(--line);padding-top:14px}.album:first-child{border-top:0;padding-top:0}.album-head{display:flex;justify-content:space-between;color:#cbd1dd;margin-bottom:8px}.songs{display:flex;gap:8px;flex-wrap:wrap}.song{padding:9px 11px}.song small{display:block;color:var(--muted);font-size:11px;margin-top:3px}.lyrics-stage{text-align:center;min-height:155px;display:grid;place-content:center;padding:18px;border-radius:18px;background:#090c13;border:1px solid var(--line)}.lyric-now{font-size:28px;font-weight:750;line-height:1.35}.lyric-next{margin-top:10px;color:var(--muted);font-size:18px}.overview{max-height:280px;overflow:auto;border-top:1px solid var(--line);padding-top:10px}.cue{padding:7px 9px;color:#8f98aa;border-radius:9px}.cue.active{color:#fff;background:#7c5cff24}.recordings{display:grid;gap:8px}.recording{border:1px solid var(--line);border-radius:14px;padding:10px}.recording audio{width:100%;margin-top:7px}.player{width:100%;margin-top:10px}.full{position:fixed;inset:0;z-index:100;background:#05070bf4;display:grid;grid-template-rows:auto 1fr;backdrop-filter:blur(20px)}.full-head{padding:20px;display:flex;justify-content:space-between;align-items:center}.full-body{overflow:auto;text-align:center;padding:8vh 9vw 30vh}.full .cue{font-size:25px;padding:12px}.full .cue.active{font-size:34px;color:#fff;background:transparent}.tag{font-size:11px;color:var(--muted);border:1px solid var(--line);border-radius:999px;padding:4px 7px}.split{display:grid;grid-template-columns:1fr 1fr;gap:12px}@media(max-width:900px){.grid,.split{grid-template-columns:1fr}.control{grid-template-columns:120px 1fr 52px}.shell{padding:14px}.lyric-now{font-size:23px}}
"#;

#[derive(Clone, Debug, Deserialize)]
struct GeneralRecording {
    id: String,
    file: String,
    bytes: u64,
    modified: String,
    #[allow(dead_code)]
    meta: Option<Value>,
}

#[component]
fn App() -> impl IntoView {
    let path = web_sys::window().and_then(|w| w.location().pathname().ok()).unwrap_or_default();
    let karaoke = path.contains("/karaoke");
    view! {
        <style>{STYLE}</style>
        <Show when=move || karaoke fallback=move || view! { <VoiceConsole/> }>
            <KaraokeConsole/>
        </Show>
    }
}

#[component]
fn VoiceConsole() -> impl IntoView {
    let devices = RwSignal::new(Vec::<DeviceOption>::new());
    let device = RwSignal::new(String::new());
    let aec = RwSignal::new(true);
    let afs = RwSignal::new(true);
    let monitor_on = RwSignal::new(false);
    let record_gain = RwSignal::new(170_u16);
    let monitor_gain = RwSignal::new(100_u16);
    let meter = RwSignal::new(0_u16);
    let mic_open = RwSignal::new(false);
    let recording = RwSignal::new(false);
    let status = RwSignal::new("Rust/WASM 控制台就绪".to_string());
    let recordings = RwSignal::new(Vec::<GeneralRecording>::new());

    refresh_devices(devices, device);
    refresh_general_recordings(recordings);
    Interval::new(100, move || meter.set(audio::meter_percent())).forget();

    let open_mic = move |_| {
        let chosen = device.get();
        let use_aec = aec.get();
        status.set("正在请求麦克风权限…".into());
        spawn_local(async move {
            match audio::open((!chosen.is_empty()).then_some(chosen), use_aec).await {
                Ok(()) => {
                    audio::set_record_gain(record_gain.get_untracked());
                    audio::set_monitor_gain(monitor_gain.get_untracked());
                    audio::set_monitor_enabled(monitor_on.get_untracked());
                    audio::set_afs_enabled(afs.get_untracked());
                    mic_open.set(true);
                    status.set("麦克风已接入 Rust/WASM Audio Graph".into());
                    refresh_devices(devices, device);
                    api::telemetry("麦克风打开", json!({"surface":"voice","aec":use_aec}));
                }
                Err(e) => status.set(format!("麦克风失败：{e}")),
            }
        });
    };

    let toggle_record = move |_| {
        if recording.get() {
            if let Err(e) = audio::stop_recording() { status.set(format!("停止录音失败：{e}")); }
            else { status.set("正在收尾并保存录音…".into()); }
            return;
        }
        if !audio::is_open() { status.set("请先打开麦克风".into()); return; }
        let done: Rc<dyn Fn(Result<(), String>)> = Rc::new(move |result| {
            recording.set(false);
            match result { Ok(()) => status.set("录音已保存".into()), Err(e) => status.set(format!("录音保存失败：{e}")) }
            refresh_general_recordings(recordings);
        });
        let spec = RecordingSpec {
            start_url: "/singeros/api/recordings/start".into(),
            session_base: "/singeros/api/recordings".into(),
            start_body: json!({}),
            finalize_body: json!({"surface":"rust-wasm","record_bus":"mic-only"}),
        };
        spawn_local(async move {
            match audio::start_recording(spec, done).await {
                Ok(()) => { recording.set(true); status.set("录制中 · 仅麦克风".into()); api::telemetry("录音开始", json!({"recordBus":"mic-only"})); }
                Err(e) => status.set(format!("开始录音失败：{e}")),
            }
        });
    };

    view! {
        <div class="shell">
            <div class="top"><div><div class="brand">"SingerOS-rs · Vocal Console"</div><div class="sub">"Leptos / Rust WASM / WebAudio · 0 手写业务 JavaScript"</div></div><a href="/singeros/karaoke/"><button>"进入 K 歌"</button></a></div>
            <div class="grid">
                <section class="panel stack">
                    <h2>"麦克风与返听"</h2>
                    <div class="row">
                        <select prop:value=move || device.get() on:change=move |ev| device.set(event_target_value(&ev)) disabled=move || recording.get()>
                            <option value="">"系统默认麦克风"</option>
                            {move || devices.get().into_iter().map(|d| view! { <option value=d.id>{d.label}</option> }).collect_view()}
                        </select>
                        <button on:click=open_mic disabled=move || recording.get()>"打开 / 重开麦克风"</button>
                        <button on:click=move |_| refresh_devices(devices, device)>"刷新设备"</button>
                    </div>
                    <div class="row"><label><input type="checkbox" prop:checked=move || aec.get() on:change=move |ev| aec.set(event_target_checked(&ev)) disabled=move || recording.get()/> " AEC（抑制扬声器串录）"</label><label><input type="checkbox" prop:checked=move || afs.get() on:change=move |ev| { let v=event_target_checked(&ev); afs.set(v); audio::set_afs_enabled(v); }/> " Adaptive feedback suppression"</label></div>
                    <div class="row"><label><input type="checkbox" prop:checked=move || monitor_on.get() on:change=move |ev| { let v=event_target_checked(&ev); monitor_on.set(v); audio::set_monitor_enabled(v); }/> " 低延迟实时返听"</label></div>
                    <div class="control"><label>"人声录制增益"</label><input type="range" min="0" max="300" prop:value=move || record_gain.get() on:input=move |ev| { let v=event_target_value(&ev).parse().unwrap_or(170); record_gain.set(v); audio::set_record_gain(v); }/><output>{move || format!("{}%", record_gain.get())}</output></div>
                    <div class="control"><label>"实时返听增益"</label><input type="range" min="0" max="200" prop:value=move || monitor_gain.get() on:input=move |ev| { let v=event_target_value(&ev).parse().unwrap_or(100); monitor_gain.set(v); audio::set_monitor_gain(v); }/><output>{move || format!("{}%", monitor_gain.get())}</output></div>
                    <div class="meter"><i style:width=move || format!("{}%", meter.get())></i></div>
                    <div class="route">"仅麦克风 → 人声增益 → Limiter → MediaRecorder；不存在歌曲输入"</div>
                    <div class="row"><button class:danger=move || recording.get() class:primary=move || !recording.get() on:click=toggle_record>{move || if recording.get() { "停止并保存" } else { "开始录音" }}</button><span class="tag">{move || if mic_open.get() { "MIC ONLINE" } else { "MIC CLOSED" }}</span></div>
                    <div class="status">{move || status.get()}</div>
                </section>
                <section class="panel"><h2>"历史录音"</h2><div class="recordings">{move || recordings.get().into_iter().map(|r| { let url=format!("/singeros/api/recordings/{}/audio",r.id); view! { <div class="recording"><div>{r.file}</div><small class="sub">{format!("{} · {} bytes",r.modified,r.bytes)}</small><audio controls preload="none" src=url></audio></div> } }).collect_view()}</div></section>
            </div>
        </div>
    }
}

#[component]
fn KaraokeConsole() -> impl IntoView {
    let catalog = RwSignal::new(None::<KaraokeCatalog>);
    let recordings = RwSignal::new(Vec::<KaraokeRecording>::new());
    let selected = RwSignal::new(String::new());
    let mode = RwSignal::new("original".to_string());
    let song_gain = RwSignal::new(35_u16);
    let record_gain = RwSignal::new(170_u16);
    let monitor_gain = RwSignal::new(100_u16);
    let aec = RwSignal::new(true);
    let afs = RwSignal::new(true);
    let monitor_on = RwSignal::new(false);
    let devices = RwSignal::new(Vec::<DeviceOption>::new());
    let device = RwSignal::new(String::new());
    let mic_open = RwSignal::new(false);
    let recording = RwSignal::new(false);
    let status = RwSignal::new("加载歌库中…".to_string());
    let meter = RwSignal::new(0_u16);
    let lyric_index = RwSignal::new(0_usize);
    let full_lyrics = RwSignal::new(false);

    refresh_devices(devices, device);
    refresh_karaoke_recordings(recordings);
    spawn_local(async move {
        match api::get_json::<KaraokeCatalog>("/singeros/api/karaoke/catalog").await {
            Ok(c) => {
                if let Some(first) = c.songs.first() { selected.set(first.id.clone()); }
                status.set(format!("歌库已加载 · {} 首", c.songs.len()));
                catalog.set(Some(c));
            }
            Err(e) => status.set(format!("歌库加载失败：{e}")),
        }
    });

    Effect::new(move |_| {
        let id = selected.get();
        let m = mode.get();
        let c = catalog.get();
        let Some(song) = c.as_ref().and_then(|x| x.songs.iter().find(|s| s.id == id)) else { return; };
        let Some(player) = song_player() else { return; };
        let key = format!("{}:{m}", song.id);
        if let Some(track) = song.tracks.get(&m) {
            if player.get_attribute("data-track-key").as_deref() != Some(&key) {
                player.set_src(&track.url);
                let _ = player.set_attribute("data-track-key", &key);
                player.set_current_time(0.0);
                lyric_index.set(0);
            }
        } else {
            player.remove_attribute("src");
            let _ = player.remove_attribute("data-track-key");
            player.load();
        }
    });
    Effect::new(move |_| if let Some(player)=song_player() { player.set_volume(song_gain.get() as f64 / 100.0); });
    Interval::new(100, move || {
        meter.set(audio::meter_percent());
        let Some(player) = song_player() else { return; };
        let Some(song) = current_song(catalog.get_untracked(), &selected.get_untracked()) else { return; };
        let (lyrics, offset) = lyrics_for(&song, &mode.get_untracked());
        if lyrics.is_empty() { return; }
        let t = player.current_time() - offset;
        let idx = lyrics.iter().position(|c| t >= c.start && t < c.end)
            .or_else(|| lyrics.iter().rposition(|c| c.start <= t)).unwrap_or(0);
        lyric_index.set(idx);
    }).forget();

    let full_for_key = full_lyrics;
    let key_cb = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
        if event.key() == "Escape" || event.key() == "Backspace" { full_for_key.set(false); }
    });
    if let Some(window) = web_sys::window() { let _ = window.add_event_listener_with_callback("keydown", key_cb.as_ref().unchecked_ref()); }
    key_cb.forget();

    let open_mic = move |_| {
        let chosen = device.get();
        let use_aec = aec.get();
        status.set("正在建立麦克风 Audio Graph…".into());
        spawn_local(async move {
            match audio::open((!chosen.is_empty()).then_some(chosen), use_aec).await {
                Ok(()) => {
                    audio::set_record_gain(record_gain.get_untracked());
                    audio::set_monitor_gain(monitor_gain.get_untracked());
                    audio::set_monitor_enabled(monitor_on.get_untracked());
                    audio::set_afs_enabled(afs.get_untracked());
                    mic_open.set(true);
                    status.set("麦克风已就绪 · 录音总线为纯人声".into());
                    refresh_devices(devices, device);
                    api::telemetry("K歌麦克风打开", json!({"aec":use_aec}));
                }
                Err(e) => status.set(format!("麦克风失败：{e}")),
            }
        });
    };

    let toggle_record = move |_| {
        if recording.get() {
            if let Some(player)=song_player() { let _=player.pause(); }
            if let Err(e)=audio::stop_recording() { status.set(format!("停止录制失败：{e}")); } else { status.set("正在写入最后分片并保存…".into()); }
            return;
        }
        if !audio::is_open() { status.set("请先打开麦克风".into()); return; }
        let Some(song)=current_song(catalog.get(), &selected.get()) else { status.set("请选择歌曲".into()); return; };
        let m=mode.get();
        if !song.tracks.contains_key(&m) { status.set("当前模式尚未提供音轨".into()); return; }
        if let Some(player)=song_player() { player.set_current_time(0.0); let _=player.play(); }
        let done: Rc<dyn Fn(Result<(), String>)> = Rc::new(move |result| {
            recording.set(false);
            match result { Ok(()) => status.set("K歌录音已保存 · 仅人声".into()), Err(e) => status.set(format!("K歌保存失败：{e}")) }
            refresh_karaoke_recordings(recordings);
        });
        let spec=RecordingSpec {
            start_url:"/singeros/api/karaoke/recordings/start".into(),
            session_base:"/singeros/api/karaoke/recordings".into(),
            start_body:json!({"song_id":song.id,"song_title":song.title,"mode":m}),
            finalize_body:json!({"record_bus":"mic-only","record_gain":record_gain.get(),"song_gain":song_gain.get(),"aec":aec.get()}),
        };
        spawn_local(async move {
            match audio::start_recording(spec,done).await {
                Ok(()) => { recording.set(true); status.set(format!("录制中 · 仅人声 · {}",mode.get_untracked())); api::telemetry("K歌开始",json!({"recordBus":"mic-only"})); }
                Err(e) => status.set(format!("开始K歌失败：{e}")),
            }
        });
    };

    let play_only = move |_| if let Some(player)=song_player() { let _=player.play(); };
    let stop_on_end = move |_| if recording.get() { let _=audio::stop_recording(); status.set("歌曲结束，正在自动保存…".into()); };

    view! {
        <div class="shell">
            <div class="top"><div><div class="brand">"SingerOS-rs · Tesla Karaoke"</div><div class="sub">"全 Rust：Leptos + WASM + WebAudio · 服务器 Axum/rustls"</div></div><a href="/singeros/"><button>"录音控制台"</button></a></div>
            <div class="grid">
                <div class="stack">
                    <section class="panel"><h2>"专辑 / 歌曲"</h2><div class="albums">{move || render_albums(catalog.get(), selected, recording)}</div></section>
                    <section class="panel stack">
                        <div class="row"><button class:active=move || mode.get()=="original" on:click=move |_| if !recording.get(){mode.set("original".into())} disabled=move || recording.get()>"原唱"</button><button class:active=move || mode.get()=="accompaniment" on:click=move |_| if !recording.get(){mode.set("accompaniment".into())} disabled=move || recording.get()>"伴奏"</button><button on:click=play_only>"仅播放"</button><button on:click=move |_| { full_lyrics.set(true); api::telemetry("歌词全屏打开",json!({})); }>"歌词全屏"</button></div>
                        <audio id="song-player" class="player" controls preload="metadata" on:ended=stop_on_end></audio>
                        <div class="lyrics-stage"><div><div class="lyric-now">{move || lyric_text(catalog.get(),&selected.get(),&mode.get(),lyric_index.get())}</div><div class="lyric-next">{move || lyric_text(catalog.get(),&selected.get(),&mode.get(),lyric_index.get()+1)}</div></div></div>
                        <div class="overview">{move || render_lyrics(catalog.get(), &selected.get(), &mode.get(), lyric_index)}</div>
                    </section>
                </div>
                <div class="stack">
                    <section class="panel stack"><h2>"K歌录制台"</h2>
                        <div class="row"><select prop:value=move || device.get() on:change=move |ev| device.set(event_target_value(&ev)) disabled=move || recording.get()><option value="">"系统默认麦克风"</option>{move || devices.get().into_iter().map(|d| view!{<option value=d.id>{d.label}</option>}).collect_view()}</select><button on:click=open_mic disabled=move || recording.get()>"打开 / 重开麦克风"</button><button on:click=move |_| refresh_devices(devices,device)>"刷新"</button></div>
                        <div class="control"><label>"歌曲播放电平"</label><input type="range" min="0" max="100" prop:value=move || song_gain.get() on:input=move |ev| song_gain.set(event_target_value(&ev).parse().unwrap_or(35))/><output>{move || format!("{}%",song_gain.get())}</output></div>
                        <div class="control"><label>"人声录制增益"</label><input type="range" min="0" max="300" prop:value=move || record_gain.get() on:input=move |ev| {let v=event_target_value(&ev).parse().unwrap_or(170);record_gain.set(v);audio::set_record_gain(v);}/><output>{move || format!("{}%",record_gain.get())}</output></div>
                        <div class="control"><label>"实时返听增益"</label><input type="range" min="0" max="200" prop:value=move || monitor_gain.get() on:input=move |ev| {let v=event_target_value(&ev).parse().unwrap_or(100);monitor_gain.set(v);audio::set_monitor_gain(v);}/><output>{move || format!("{}%",monitor_gain.get())}</output></div>
                        <div class="row"><label><input type="checkbox" prop:checked=move || aec.get() on:change=move |ev| aec.set(event_target_checked(&ev)) disabled=move || recording.get()/> " AEC 防伴奏串录"</label><label><input type="checkbox" prop:checked=move || monitor_on.get() on:change=move |ev| {let v=event_target_checked(&ev);monitor_on.set(v);audio::set_monitor_enabled(v);}/> " 实时返听"</label><label><input type="checkbox" prop:checked=move || afs.get() on:change=move |ev| {let v=event_target_checked(&ev);afs.set(v);audio::set_afs_enabled(v);}/> " AFS"</label></div>
                        <div class="meter"><i style:width=move || format!("{}%",meter.get())></i></div><div class="route">"仅麦克风 → 人声增益 → Limiter → 录音；歌曲播放器与录音图在类型/拓扑上分离"</div>
                        <div class="row"><button class:primary=move || !recording.get() class:danger=move || recording.get() on:click=toggle_record>{move || if recording.get(){"停止并保存"}else{"开始K歌录制"}}</button><span class="tag">{move || if mic_open.get(){"MIC ONLINE"}else{"MIC CLOSED"}}</span></div><div class="status">{move || status.get()}</div>
                    </section>
                    <section class="panel"><h2>"K歌录音"</h2><div class="recordings">{move || recordings.get().into_iter().map(|r| view!{<div class="recording"><div>{format!("{} · {}",r.song_title,r.mode)}</div><small class="sub">{format!("{} · {}s · {} bytes",r.created_at_shanghai,r.duration_seconds,r.bytes)}</small><audio controls preload="none" src=r.url></audio></div>}).collect_view()}</div></section>
                </div>
            </div>
        </div>
        <Show when=move || full_lyrics.get() fallback=|| ()>
            <div class="full"><div class="full-head"><div><b>{move || current_song(catalog.get(),&selected.get()).map(|s|s.title).unwrap_or_default()}</b><div class="sub">"Esc / Backspace 返回"</div></div><button on:click=move |_| {full_lyrics.set(false);api::telemetry("歌词全屏关闭",json!({}));}>"返回"</button></div><div class="full-body">{move || render_lyrics(catalog.get(),&selected.get(),&mode.get(),lyric_index)}</div></div>
        </Show>
    }
}

fn refresh_devices(devices: RwSignal<Vec<DeviceOption>>, selected: RwSignal<String>) {
    spawn_local(async move {
        if let Ok(list)=audio::enumerate_inputs().await {
            if selected.get_untracked().is_empty() { if let Some(first)=list.first(){ selected.set(first.id.clone()); } }
            devices.set(list);
        }
    });
}

fn refresh_general_recordings(target: RwSignal<Vec<GeneralRecording>>) { spawn_local(async move { if let Ok(v)=api::get_json("/singeros/api/recordings").await{target.set(v)} }); }
fn refresh_karaoke_recordings(target: RwSignal<Vec<KaraokeRecording>>) { spawn_local(async move { if let Ok(v)=api::get_json("/singeros/api/karaoke/recordings").await{target.set(v)} }); }
fn song_player() -> Option<HtmlAudioElement> { web_sys::window()?.document()?.get_element_by_id("song-player")?.dyn_into().ok() }
fn current_song(catalog: Option<KaraokeCatalog>, id: &str) -> Option<KaraokeSong> { catalog?.songs.into_iter().find(|s| s.id == id) }
fn lyrics_for(song: &KaraokeSong, mode: &str) -> (Vec<KaraokeCue>, f64) {
    let track=song.tracks.get(mode);
    let lyrics=if !song.lyrics.is_empty(){song.lyrics.clone()}else{track.map(|t|t.lyrics.clone()).unwrap_or_default()};
    let offset=track.map(|t|t.lyrics_offset).unwrap_or(0.0);
    (lyrics,offset)
}
fn lyric_text(catalog: Option<KaraokeCatalog>, id: &str, mode: &str, index: usize) -> String {
    let Some(song)=current_song(catalog,id) else{return "—".into()};
    lyrics_for(&song,mode).0.get(index).map(|c|c.text.clone()).unwrap_or_else(||"—".into())
}
fn render_lyrics(catalog: Option<KaraokeCatalog>, id: &str, mode: &str, current: RwSignal<usize>) -> impl IntoView {
    let lyrics=current_song(catalog,id).map(|s|lyrics_for(&s,mode).0).unwrap_or_default();
    lyrics.into_iter().enumerate().map(|(i,c)|view!{<div class="cue" class:active=move || current.get()==i>{c.text}</div>}).collect_view()
}
fn render_albums(catalog: Option<KaraokeCatalog>, selected: RwSignal<String>, recording: RwSignal<bool>) -> impl IntoView {
    let Some(c)=catalog else{return view!{<div class="sub">"歌库不可用"</div>}.into_any()};
    let songs=c.songs.clone();
    c.albums.into_iter().map(move |album|{
        let album_songs=songs.iter().filter(|s|s.album==album.title).cloned().collect::<Vec<_>>();
        view!{<div class="album"><div class="album-head"><b>{album.title.clone()}</b><span>{format!("{} · {}",album.year,album.language)}</span></div><div class="songs">{album_songs.into_iter().map(|song|{let id=song.id.clone();let original=song.tracks.contains_key("original");let accompaniment=song.tracks.contains_key("accompaniment");view!{<button class="song" class:active=move || selected.get()==id disabled=move || recording.get() on:click=move |_| if !recording.get(){selected.set(song.id.clone())}>{song.title.clone()}<small>{format!("{} · 原唱{} · 伴奏{} · 歌词{}",song.language,if original{"✓"}else{"待补"},if accompaniment{"✓"}else{"待补"},if !song.lyrics.is_empty(){"✓"}else{"待校准"})}</small></button>}}).collect_view()}</div></div>}
    }).collect_view().into_any()
}

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
