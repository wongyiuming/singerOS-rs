use crate::api;
use gloo_timers::{callback::Interval, future::TimeoutFuture};
use js_sys::{Date, Object, Reflect, Uint8Array};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{cell::{Cell, RefCell}, rc::Rc};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    AnalyserNode, AudioContext, BiquadFilterNode, BiquadFilterType, BlobEvent,
    DynamicsCompressorNode, GainNode, MediaDeviceInfo, MediaDeviceKind, MediaRecorder,
    MediaStream, MediaStreamAudioDestinationNode, MediaStreamTrack,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceOption {
    pub id: String,
    pub label: String,
}

struct AudioEngine {
    context: AudioContext,
    stream: MediaStream,
    record_gain: GainNode,
    monitor_gain: GainNode,
    monitor_level: Rc<Cell<f32>>,
    monitor_enabled: Rc<Cell<bool>>,
    analyser: AnalyserNode,
    notch: BiquadFilterNode,
    afs_enabled: Rc<Cell<bool>>,
    record_destination: MediaStreamAudioDestinationNode,
    _afs_loop: Interval,
}

struct RecordingRuntime {
    recorder: MediaRecorder,
    _data: Closure<dyn FnMut(BlobEvent)>,
    _stop: Closure<dyn FnMut(web_sys::Event)>,
}

#[derive(Clone)]
pub struct RecordingSpec {
    pub start_url: String,
    pub session_base: String,
    pub start_body: Value,
    pub finalize_body: Value,
}

#[derive(Deserialize)]
struct StartResponse { id: String }

thread_local! {
    static ENGINE: RefCell<Option<AudioEngine>> = const { RefCell::new(None) };
    static RECORDING: RefCell<Option<RecordingRuntime>> = const { RefCell::new(None) };
}

pub async fn enumerate_inputs() -> Result<Vec<DeviceOption>, String> {
    let window = web_sys::window().ok_or("window unavailable")?;
    let media = window.navigator().media_devices().map_err(js_error)?;
    let promise = media.enumerate_devices().map_err(js_error)?;
    let list = JsFuture::from(promise).await.map_err(js_error)?;
    let array = js_sys::Array::from(&list);
    let mut out = Vec::new();
    for value in array.iter() {
        let Ok(info) = value.dyn_into::<MediaDeviceInfo>() else { continue; };
        if info.kind() == MediaDeviceKind::Audioinput {
            let id = info.device_id();
            let label = if info.label().is_empty() { format!("麦克风 {}", out.len() + 1) } else { info.label() };
            out.push(DeviceOption { id, label });
        }
    }
    Ok(out)
}

pub async fn open(device_id: Option<String>, echo_cancellation: bool) -> Result<(), String> {
    close();
    let window = web_sys::window().ok_or("window unavailable")?;
    let media = window.navigator().media_devices().map_err(js_error)?;
    let audio = Object::new();
    Reflect::set(&audio, &"echoCancellation".into(), &JsValue::from_bool(echo_cancellation)).map_err(js_error)?;
    Reflect::set(&audio, &"noiseSuppression".into(), &JsValue::FALSE).map_err(js_error)?;
    Reflect::set(&audio, &"autoGainControl".into(), &JsValue::FALSE).map_err(js_error)?;
    Reflect::set(&audio, &"sampleRate".into(), &JsValue::from_f64(48_000.0)).map_err(js_error)?;
    if let Some(id) = device_id.filter(|x| !x.is_empty()) {
        let exact = Object::new();
        Reflect::set(&exact, &"exact".into(), &JsValue::from_str(&id)).map_err(js_error)?;
        Reflect::set(&audio, &"deviceId".into(), &exact).map_err(js_error)?;
    }
    let raw = Object::new();
    Reflect::set(&raw, &"audio".into(), &audio).map_err(js_error)?;
    Reflect::set(&raw, &"video".into(), &JsValue::FALSE).map_err(js_error)?;
    let constraints = raw.unchecked_into();
    let promise = media.get_user_media_with_constraints(&constraints).map_err(js_error)?;
    let stream: MediaStream = JsFuture::from(promise).await.map_err(js_error)?.dyn_into().map_err(js_error)?;

    let context = AudioContext::new().map_err(js_error)?;
    if let Ok(promise) = context.resume() { let _ = JsFuture::from(promise).await; }
    let source = context.create_media_stream_source(&stream).map_err(js_error)?;

    let record_gain = context.create_gain().map_err(js_error)?;
    record_gain.gain().set_value(1.7);
    let limiter = context.create_dynamics_compressor().map_err(js_error)?;
    limiter.threshold().set_value(-1.0);
    limiter.knee().set_value(0.0);
    limiter.ratio().set_value(20.0);
    limiter.attack().set_value(0.001);
    limiter.release().set_value(0.08);
    let analyser = context.create_analyser().map_err(js_error)?;
    analyser.set_fft_size(1024);
    let record_destination = context.create_media_stream_destination().map_err(js_error)?;
    source.connect_with_audio_node(&record_gain).map_err(js_error)?;
    record_gain.connect_with_audio_node(&limiter).map_err(js_error)?;
    limiter.connect_with_audio_node(&analyser).map_err(js_error)?;
    analyser.connect_with_audio_node(&record_destination).map_err(js_error)?;

    let highpass = context.create_biquad_filter().map_err(js_error)?;
    highpass.set_type(BiquadFilterType::Highpass);
    highpass.frequency().set_value(75.0);
    let notch = context.create_biquad_filter().map_err(js_error)?;
    notch.set_type(BiquadFilterType::Notch);
    notch.frequency().set_value(1000.0);
    notch.q().set_value(12.0);
    let compressor: DynamicsCompressorNode = context.create_dynamics_compressor().map_err(js_error)?;
    compressor.threshold().set_value(-9.0);
    compressor.knee().set_value(8.0);
    compressor.ratio().set_value(6.0);
    compressor.attack().set_value(0.003);
    compressor.release().set_value(0.12);
    let monitor_gain = context.create_gain().map_err(js_error)?;
    monitor_gain.gain().set_value(0.0);
    source.connect_with_audio_node(&highpass).map_err(js_error)?;
    highpass.connect_with_audio_node(&notch).map_err(js_error)?;
    notch.connect_with_audio_node(&compressor).map_err(js_error)?;
    compressor.connect_with_audio_node(&monitor_gain).map_err(js_error)?;
    monitor_gain.connect_with_audio_node(&context.destination()).map_err(js_error)?;

    let monitor_level = Rc::new(Cell::new(1.0));
    let monitor_enabled = Rc::new(Cell::new(false));
    let afs_enabled = Rc::new(Cell::new(true));
    let a = analyser.clone();
    let n = notch.clone();
    let enabled = afs_enabled.clone();
    let sample_rate = context.sample_rate();
    let afs_loop = Interval::new(250, move || {
        if !enabled.get() { return; }
        let mut bins = vec![0_u8; a.frequency_bin_count() as usize];
        a.get_byte_frequency_data(&mut bins);
        let Some((idx, peak)) = bins.iter().copied().enumerate().max_by_key(|(_, v)| *v) else { return; };
        if peak < 232 { return; }
        let hz = idx as f32 * sample_rate / a.fft_size() as f32;
        if (170.0..=8000.0).contains(&hz) { n.frequency().set_value(hz); n.q().set_value(14.0); }
    });

    ENGINE.with(|slot| *slot.borrow_mut() = Some(AudioEngine {
        context, stream, record_gain, monitor_gain, monitor_level, monitor_enabled,
        analyser, notch, afs_enabled, record_destination, _afs_loop: afs_loop,
    }));
    Ok(())
}

pub fn close() {
    RECORDING.with(|slot| {
        if let Some(r) = slot.borrow().as_ref() { let _ = r.recorder.stop(); }
        *slot.borrow_mut() = None;
    });
    ENGINE.with(|slot| {
        if let Some(engine) = slot.borrow_mut().take() {
            for track in engine.stream.get_tracks().iter() {
                if let Ok(track) = track.dyn_into::<MediaStreamTrack>() { track.stop(); }
            }
            let _ = engine.context.close();
        }
    });
}

pub fn is_open() -> bool { ENGINE.with(|slot| slot.borrow().is_some()) }

pub fn set_record_gain(percent: u16) {
    ENGINE.with(|slot| if let Some(e) = slot.borrow().as_ref() { e.record_gain.gain().set_value((percent.min(300) as f32) / 100.0); });
}

pub fn set_monitor_gain(percent: u16) {
    ENGINE.with(|slot| if let Some(e) = slot.borrow().as_ref() {
        let value = (percent.min(200) as f32) / 100.0;
        e.monitor_level.set(value);
        if e.monitor_enabled.get() { e.monitor_gain.gain().set_value(value); }
    });
}

pub fn set_monitor_enabled(on: bool) {
    ENGINE.with(|slot| if let Some(e) = slot.borrow().as_ref() {
        e.monitor_enabled.set(on);
        e.monitor_gain.gain().set_value(if on { e.monitor_level.get() } else { 0.0 });
    });
}

pub fn set_afs_enabled(on: bool) {
    ENGINE.with(|slot| if let Some(e) = slot.borrow().as_ref() {
        e.afs_enabled.set(on);
        e.notch.q().set_value(if on { 14.0 } else { 0.0001 });
    });
}

pub fn meter_percent() -> u16 {
    ENGINE.with(|slot| {
        let Some(e) = slot.borrow().as_ref() else { return 0; };
        let mut samples = vec![128_u8; e.analyser.fft_size() as usize];
        e.analyser.get_byte_time_domain_data(&mut samples);
        let rms = (samples.iter().map(|v| ((*v as f64 - 128.0) / 128.0).powi(2)).sum::<f64>() / samples.len().max(1) as f64).sqrt();
        (rms * 260.0).clamp(0.0, 100.0) as u16
    })
}

pub async fn start_recording(spec: RecordingSpec, on_done: Rc<dyn Fn(Result<(), String>)>) -> Result<(), String> {
    let stream = ENGINE.with(|slot| slot.borrow().as_ref().map(|e| e.record_destination.stream())).ok_or("microphone is not open")?;
    let recorder = MediaRecorder::new_with_media_stream(&stream).map_err(js_error)?;
    let mime = if recorder.mime_type().is_empty() { "audio/webm".to_string() } else { recorder.mime_type() };
    let mut start_body = spec.start_body.clone();
    if let Some(obj) = start_body.as_object_mut() { obj.insert("mime".into(), Value::String(mime)); }
    let started: StartResponse = api::post_json(&spec.start_url, &start_body).await?;
    let id = started.id;
    let pending = Rc::new(Cell::new(0_u32));
    let next_seq = Rc::new(Cell::new(0_u64));
    let upload_error: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    let data_id = id.clone();
    let data_base = spec.session_base.clone();
    let data_pending = pending.clone();
    let data_seq = next_seq.clone();
    let data_error = upload_error.clone();
    let data_cb = Closure::<dyn FnMut(BlobEvent)>::new(move |event: BlobEvent| {
        let blob = event.data();
        if blob.size() == 0 { return; }
        let seq = data_seq.get();
        data_seq.set(seq + 1);
        data_pending.set(data_pending.get() + 1);
        let pending = data_pending.clone();
        let error = data_error.clone();
        let url = format!("{}/{}/chunk?seq={seq}", data_base, data_id);
        spawn_local(async move {
            let result = async {
                let buffer = JsFuture::from(blob.array_buffer()).await.map_err(js_error)?;
                let bytes = Uint8Array::new(&buffer).to_vec();
                api::post_bytes(&url, bytes).await
            }.await;
            if let Err(e) = result { *error.borrow_mut() = Some(e); }
            pending.set(pending.get().saturating_sub(1));
        });
    });
    recorder.set_ondataavailable(Some(data_cb.as_ref().unchecked_ref()));

    let stop_pending = pending.clone();
    let stop_error = upload_error.clone();
    let stop_id = id.clone();
    let stop_base = spec.session_base.clone();
    let stop_body = spec.finalize_body.clone();
    let stop_done = on_done.clone();
    let started_ms = Date::now();
    let stop_cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        let pending = stop_pending.clone();
        let error = stop_error.clone();
        let id = stop_id.clone();
        let base = stop_base.clone();
        let mut body = stop_body.clone();
        let done = stop_done.clone();
        spawn_local(async move {
            while pending.get() > 0 { TimeoutFuture::new(40).await; }
            if let Some(e) = error.borrow().clone() { done(Err(e)); return; }
            if let Some(obj) = body.as_object_mut() {
                obj.insert("duration_seconds".into(), json!(((Date::now() - started_ms) / 1000.0).max(0.0)));
            }
            let url = format!("{base}/{id}/finalize");
            let result: Result<Value, String> = api::post_json(&url, &body).await;
            done(result.map(|_| ())); 
        });
    });
    recorder.set_onstop(Some(stop_cb.as_ref().unchecked_ref()));
    recorder.start_with_time_slice(5000).map_err(js_error)?;
    RECORDING.with(|slot| *slot.borrow_mut() = Some(RecordingRuntime { recorder, _data: data_cb, _stop: stop_cb }));
    Ok(())
}

pub fn stop_recording() -> Result<(), String> {
    RECORDING.with(|slot| {
        let binding = slot.borrow();
        let runtime = binding.as_ref().ok_or("recording is not active")?;
        let _ = runtime.recorder.request_data();
        runtime.recorder.stop().map_err(js_error)
    })
}

fn js_error(value: impl Into<JsValue>) -> String {
    let value = value.into();
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}
