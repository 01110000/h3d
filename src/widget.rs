//! Blitz custom widget hosting the splat renderer and the orbit/zoom controls.

use crate::renderer::{SplatRenderer, create_target};
use crate::splats;
use anyrender::{PaintRef, PaintScene, RenderContext, ResourceId};
use blitz_dom::Widget;
use blitz_dom::node::ComputedStyles;
use blitz_traits::events::{BlitzPointerEvent, BlitzPointerId, BlitzWheelDelta, UiEvent};
use dioxus_native::DeviceHandle;
use glam::{Mat4, Quat, Vec2, Vec3};
use peniko::kurbo::{Affine, Rect};
use peniko::{Fill, ImageBrush, ImageSampler};
use std::sync::Arc;

const FOV_Y: f32 = 45.0 * std::f32::consts::PI / 180.0;
const MIN_DISTANCE: f32 = 1.2;
const MAX_DISTANCE: f32 = 20.0;
const ROTATE_SPEED: f32 = 0.008;

/// Trackball-style camera: the object rotates freely, the camera dollies in/out.
pub struct Camera {
    pub rotation: Quat,
    pub distance: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            rotation: Quat::from_rotation_x(0.6),
            distance: 4.2,
        }
    }
}

impl Camera {
    pub fn view(&self) -> Mat4 {
        Mat4::from_translation(Vec3::new(0.0, 0.0, -self.distance)) * Mat4::from_quat(self.rotation)
    }

    pub fn rotate(&mut self, delta_px: Vec2) {
        let yaw = Quat::from_rotation_y(delta_px.x * ROTATE_SPEED);
        let pitch = Quat::from_rotation_x(delta_px.y * ROTATE_SPEED);
        self.rotation = (yaw * pitch * self.rotation).normalize();
    }

    pub fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance / factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }
}

struct Target {
    texture: wgpu::Texture,
    handle: ResourceId,
}

pub struct SplatWidget {
    renderer: Option<SplatRenderer>,
    /// Double-buffered render targets (one displayed, one being drawn).
    targets: [Option<Target>; 2],
    front: usize,
    camera: Camera,
    pointers: Vec<(BlitzPointerId, Vec2)>,
    dirty: bool,
    window: Option<Arc<dyn dioxus_native::winit::window::Window>>,
}

impl SplatWidget {
    pub fn new(window: Option<Arc<dyn dioxus_native::winit::window::Window>>) -> Self {
        Self {
            renderer: None,
            targets: [None, None],
            front: 0,
            camera: Camera::default(),
            pointers: Vec::new(),
            dirty: true,
            window,
        }
    }

    fn changed(&mut self) {
        self.dirty = true;
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn pointer_pos(event: &BlitzPointerEvent) -> Vec2 {
        Vec2::new(event.client_x(), event.client_y())
    }

    fn on_pointer_down(&mut self, event: &BlitzPointerEvent) {
        let pos = Self::pointer_pos(event);
        self.pointers.retain(|(id, _)| *id != event.id);
        self.pointers.push((event.id, pos));
    }

    fn on_pointer_up(&mut self, event: &BlitzPointerEvent) {
        self.pointers.retain(|(id, _)| *id != event.id);
    }

    fn on_pointer_move(&mut self, event: &BlitzPointerEvent) {
        if event.id == BlitzPointerId::Mouse && event.buttons.is_empty() {
            self.pointers.retain(|(id, _)| *id != BlitzPointerId::Mouse);
            return;
        }
        let Some(index) = self.pointers.iter().position(|(id, _)| *id == event.id) else {
            return;
        };
        let pos = Self::pointer_pos(event);
        let prev = self.pointers[index].1;

        if self.pointers.len() >= 2 {
            // Pinch to zoom (two fingers), and drag the midpoint to rotate.
            let other = self.pointers[if index == 0 { 1 } else { 0 }].1;
            let (before, after) = (prev.distance(other), pos.distance(other));
            if before > 1.0 && after > 1.0 {
                self.camera.zoom(after / before);
            }
            self.camera.rotate((pos - prev) * 0.5);
        } else {
            self.camera.rotate(pos - prev);
        }
        self.pointers[index].1 = pos;
        self.changed();
    }

    fn on_wheel(&mut self, delta: &BlitzWheelDelta) {
        let dy = match *delta {
            BlitzWheelDelta::Lines(_, y) => y as f32 * 40.0,
            BlitzWheelDelta::Pixels(_, y) => y as f32,
        };
        self.camera.zoom((dy * 0.002).exp());
        self.changed();
    }
}

impl Widget for SplatWidget {
    fn can_create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        let Some(ctx) = render_ctx.renderer_specific_context() else {
            eprintln!("H3D: renderer returned no context; 3DGS requires a wgpu backend");
            return;
        };
        match ctx.downcast::<DeviceHandle>() {
            Ok(handle) => {
                let splats = splats::white_torus();
                self.renderer = Some(SplatRenderer::new(&handle.device, &handle.queue, splats));
                self.dirty = true;
            }
            Err(_) => eprintln!("H3D: non-wgpu rendering backend, cannot draw splats"),
        }
    }

    fn destroy_surfaces(&mut self) {
        self.renderer = None;
        self.targets = [None, None];
    }

    fn requires_redraw(&self) -> bool {
        self.dirty
    }

    fn handle_event(&mut self, event: &UiEvent) {
        match event {
            UiEvent::PointerDown(e) => self.on_pointer_down(e),
            UiEvent::PointerMove(e) => self.on_pointer_move(e),
            UiEvent::PointerUp(e) | UiEvent::PointerCancel(e) | UiEvent::PointerLeave(e) => {
                self.on_pointer_up(e)
            }
            UiEvent::Wheel(e) => self.on_wheel(&e.delta),
            _ => {}
        }
    }

    fn paint(
        &mut self,
        render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        _scale: f64,
    ) -> anyrender::Scene {
        let mut scene = anyrender::Scene::new();
        self.dirty = false;
        if width == 0 || height == 0 {
            return scene;
        }
        let Some(renderer) = &mut self.renderer else {
            return scene;
        };

        let back = 1 - self.front;
        if let Some(target) = &self.targets[back]
            && (target.texture.width() != width || target.texture.height() != height)
        {
            render_ctx.unregister_resource(self.targets[back].take().unwrap().handle);
        }
        if self.targets[back].is_none() {
            let texture = create_target(renderer.device(), width, height);
            let Ok(handle) = render_ctx.try_register_custom_resource(Box::new(texture.clone()))
            else {
                return scene;
            };
            self.targets[back] = Some(Target { texture, handle });
        }
        let target = self.targets[back].as_ref().unwrap();
        renderer.render(&target.texture, self.camera.view(), FOV_Y);

        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintRef::Resource(ImageBrush {
                image: target.handle,
                sampler: ImageSampler::default(),
            }),
            None,
            &Rect::from_origin_size((0.0, 0.0), (width as f64, height as f64)),
        );
        self.front = back;
        scene
    }
}
