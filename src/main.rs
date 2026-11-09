mod renderer;
mod splats;
mod widget;

use dioxus::prelude::*;
use dioxus_native::{CustomWidgetAttr, LogicalSize, WindowAttributes};
use std::any::Any;
use widget::SplatWidget;

const STYLE: &str = "html, body { margin: 0; padding: 0; width: 100%; height: 100%; \
    background: #000; overflow: hidden; }";

fn main() {
    let window = WindowAttributes::default()
        .with_title("H3D")
        .with_surface_size(LogicalSize::new(1024, 768));
    let config: Vec<Box<dyn Any>> = vec![Box::new(window)];
    dioxus_native::launch_cfg(app, Vec::new(), config);
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: dioxus_native::AndroidApp) {
    dioxus_native::set_android_app(app);
    main();
}

fn app() -> Element {
    let window = dioxus_native::use_window();
    let splat_view = use_hook(move || CustomWidgetAttr::new(SplatWidget::new(Some(window))));

    rsx! {
        style { {STYLE} }
        object {
            "data": splat_view,
            style: "display: block; width: 100vw; height: 100vh; background: #000;",
        }
    }
}
