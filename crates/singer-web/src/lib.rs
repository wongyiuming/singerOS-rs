#[cfg(target_arch = "wasm32")]
mod wasm_app {
    use leptos::prelude::*;
    use wasm_bindgen::prelude::*;

    #[component]
    fn App() -> impl IntoView {
        let song_gain = RwSignal::new(35_u16);
        let record_gain = RwSignal::new(170_u16);

        view! {
            <main>
                <h1>"SingerOS-rs"</h1>
                <p>"Rust/WASM · zero handwritten business JavaScript"</p>
                <label>"Song " {move || song_gain.get()} "%"</label>
                <input type="range" min="0" max="100"
                    prop:value=move || song_gain.get()
                    on:input=move |ev| song_gain.set(event_target_value(&ev).parse().unwrap_or(35)) />
                <label>"Voice record " {move || record_gain.get()} "%"</label>
                <input type="range" min="0" max="300"
                    prop:value=move || record_gain.get()
                    on:input=move |ev| record_gain.set(event_target_value(&ev).parse().unwrap_or(170)) />
            </main>
        }
    }

    #[wasm_bindgen(start)]
    pub fn start() {
        mount_to_body(App);
    }
}
