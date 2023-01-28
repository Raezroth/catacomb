//! Layer shell windows.

use smithay::backend::renderer::gles2::Gles2Frame;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Physical, Point, Rectangle};
use smithay::wayland::shell::wlr_layer::Layer;

use crate::output::Canvas;
use crate::windows::surface::CatacombLayerSurface;
use crate::windows::window::Window;
use crate::windows::OpaqueRegions;

type LayerWindow = Window<CatacombLayerSurface>;

/// Layer shell windows.
#[derive(Debug, Default)]
pub struct Layers {
    pub focus: Option<WlSurface>,

    background: Vec<LayerWindow>,
    bottom: Vec<LayerWindow>,
    top: Vec<LayerWindow>,
    overlay: Vec<LayerWindow>,
}

impl Layers {
    /// Add a new layer shell window.
    pub fn add(&mut self, layer: Layer, window: LayerWindow) {
        match layer {
            Layer::Background => self.background.push(window),
            Layer::Bottom => self.bottom.push(window),
            Layer::Top => self.top.push(window),
            Layer::Overlay => self.overlay.push(window),
        }
    }

    /// Iterate over all layer shell windows.
    pub fn iter(&self) -> impl Iterator<Item = &LayerWindow> {
        self.background.iter().chain(&self.bottom).chain(&self.top).chain(&self.overlay)
    }

    /// Iterate mutably over all layer shell windows.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut LayerWindow> {
        self.background
            .iter_mut()
            .chain(&mut self.bottom)
            .chain(&mut self.top)
            .chain(&mut self.overlay)
    }

    /// Iterate over layer shell background windows, bottom to top.
    pub fn background(&self) -> impl Iterator<Item = &LayerWindow> {
        self.background.iter().chain(&self.bottom)
    }

    /// Iterate over layer shell foreground windows, bottom to top.
    pub fn foreground(&self) -> impl Iterator<Item = &LayerWindow> {
        self.top.iter().chain(&self.overlay)
    }

    /// Iterate over layer shell overlay windows, bottom to top.
    pub fn overlay(&self) -> impl Iterator<Item = &LayerWindow> {
        self.overlay.iter()
    }

    /// Iterate over layer shell overlay windows, bottom to top.
    pub fn overlay_mut(&mut self) -> impl Iterator<Item = &mut LayerWindow> {
        self.overlay.iter_mut()
    }

    /// Draw background/bottom layer windows.
    pub fn draw_background(
        &mut self,
        frame: &mut Gles2Frame,
        canvas: &Canvas,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &mut OpaqueRegions,
    ) {
        for window in &mut self.background {
            window.draw(frame, canvas, 1., None, None, damage, &mut *opaque_regions);
        }

        for window in &mut self.bottom {
            window.draw(frame, canvas, 1., None, None, damage, &mut *opaque_regions);
        }
    }

    /// Draw top/overlay layer windows.
    pub fn draw_foreground(
        &mut self,
        frame: &mut Gles2Frame,
        canvas: &Canvas,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &mut OpaqueRegions,
    ) {
        for window in &mut self.top {
            window.draw(frame, canvas, 1., None, None, damage, &mut *opaque_regions);
        }

        self.draw_overlay(frame, canvas, damage, opaque_regions);
    }

    /// Draw overlay layer windows.
    pub fn draw_overlay(
        &mut self,
        frame: &mut Gles2Frame,
        canvas: &Canvas,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &mut OpaqueRegions,
    ) {
        for window in &mut self.overlay {
            window.draw(frame, canvas, 1., None, None, damage, &mut *opaque_regions);
        }
    }

    /// Request new frames from all layer shell windows.
    pub fn request_frames(&mut self, runtime: u32) {
        for window in self.iter_mut() {
            window.request_frame(runtime);
        }
    }

    /// Foreground window at the specified position.
    pub fn foreground_window_at(&self, position: Point<f64, Logical>) -> Option<&LayerWindow> {
        self.overlay
            .iter()
            .rev()
            .find(|window| window.contains(position))
            .or_else(|| self.top.iter().rev().find(|window| window.contains(position)))
    }

    /// Background window at the specified position.
    pub fn background_window_at(&self, position: Point<f64, Logical>) -> Option<&LayerWindow> {
        self.bottom
            .iter()
            .rev()
            .find(|window| window.contains(position))
            .or_else(|| self.background.iter().rev().find(|window| window.contains(position)))
    }

    /// Overlay window at the specified position.
    pub fn overlay_window_at(&self, position: Point<f64, Logical>) -> Option<&LayerWindow> {
        self.overlay.iter().rev().find(|window| window.contains(position))
    }

    /// Apply all pending transactional updates.
    pub fn apply_transaction(&mut self) {
        Self::apply_window_transactions(&mut self.background);
        Self::apply_window_transactions(&mut self.bottom);
        Self::apply_window_transactions(&mut self.top);
        Self::apply_window_transactions(&mut self.overlay);
    }

    /// Apply transactions to all windows and remove dead ones.
    fn apply_window_transactions(windows: &mut Vec<LayerWindow>) {
        for i in (0..windows.len()).rev() {
            if windows[i].alive() {
                windows[i].apply_transaction();
            } else {
                windows.remove(i);
            }
        }
    }
}
