mod renderer;
mod splats;
mod widget;

use dioxus::prelude::*;
#[cfg(not(target_os = "ios"))]
use dioxus_native::LogicalSize;
use dioxus_native::{CustomWidgetAttr, WindowAttributes};
use std::any::Any;
use widget::SplatWidget;

const STYLE: &str = "html, body { margin: 0; padding: 0; width: 100%; height: 100%; \
    background: #000; overflow: hidden; }";

fn main() {
    let window = WindowAttributes::default().with_title("H3D");
    // On iOS winit uses the surface size as the UIWindow frame, so leave it unset to fill the screen.
    #[cfg(not(target_os = "ios"))]
    let window = window.with_surface_size(LogicalSize::new(1024, 768));
    #[allow(unused_mut)]
    let mut config: Vec<Box<dyn Any>> = vec![Box::new(window)];
    // The page background only covers the safe area on iOS; clear the status bar and
    // home indicator regions to black too instead of the renderer's default white.
    #[cfg(target_os = "ios")]
    config.push(Box::new(
        dioxus_native::Config::new().with_base_color(dioxus_native::Color::BLACK),
    ));
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
