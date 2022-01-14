//! Drawing utilities.

use std::ops::Deref;
use std::rc::Rc;

use smithay::backend::renderer;
use smithay::backend::renderer::gles2::{ffi, Gles2Error, Gles2Frame, Gles2Renderer, Gles2Texture};
use smithay::backend::renderer::{Frame, Transform};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::utils::{Logical, Physical, Point, Rectangle, Size};
use smithay::wayland::compositor::{BufferAssignment, SurfaceAttributes};

use crate::geometry::Vector;
use crate::output::Output;
use crate::overview::FG_OVERVIEW_PERCENTAGE;

/// Color of the hovered overview tiling location highlight.
const ACTIVE_DROP_TARGET_RGBA: [u8; 4] = [128, 128, 128, 128];

/// Color of the overview tiling location highlight.
const DROP_TARGET_RGBA: [u8; 4] = [128, 128, 128, 64];

/// Background color behind half-size windows in the overview.
const BACKGROUND_RGBA: [u8; 4] = [0, 0, 0, 255];

/// Decoration titlebar color in the overview.
const TITLE_RGBA: [u8; 4] = [64, 64, 64, 255];

/// Decoration border color in the overview.
const BORDER_RGBA: [u8; 4] = [32, 32, 32, 255];

/// Height of the window decoration title in the application overview with a DPR of 1.
const OVERVIEW_TITLE_HEIGHT: i32 = 30;

/// Width of the window decoration border in the application overview with a DPR of 1.
const OVERVIEW_BORDER_WIDTH: i32 = 1;

/// Cached texture.
///
/// Includes all information necessary to render a surface's texture even after
/// the surface itself has already died.
#[derive(Clone, Debug)]
pub struct Texture {
    size: Size<i32, Logical>,
    location: Point<i32, Logical>,
    texture: Rc<Gles2Texture>,
    scale: i32,
}

impl Texture {
    pub fn new(
        texture: Rc<Gles2Texture>,
        size: impl Into<Size<i32, Logical>>,
        location: impl Into<Point<i32, Logical>>,
        scale: i32,
    ) -> Self {
        Self { size: size.into(), location: location.into(), texture, scale }
    }

    /// Create a texture from an RGBA buffer.
    pub fn from_buffer(
        renderer: &mut Gles2Renderer,
        buffer: &[u8],
        width: i32,
        height: i32,
    ) -> Result<Self, Gles2Error> {
        assert!(buffer.len() as i32 >= width * height * 4);

        let texture = renderer.with_context(|renderer, gl| unsafe {
            let mut tex = 0;
            gl.GenTextures(1, &mut tex);
            gl.BindTexture(ffi::TEXTURE_2D, tex);
            gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_WRAP_S, ffi::CLAMP_TO_EDGE as i32);
            gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_WRAP_T, ffi::CLAMP_TO_EDGE as i32);
            gl.TexImage2D(
                ffi::TEXTURE_2D,
                0,
                ffi::RGBA as i32,
                width,
                height,
                0,
                ffi::RGBA,
                ffi::UNSIGNED_BYTE as u32,
                buffer.as_ptr() as *const _,
            );
            gl.BindTexture(ffi::TEXTURE_2D, 0);

            Gles2Texture::from_raw(renderer, tex, (width, height).into())
        })?;

        Ok(Texture::new(Rc::new(texture), (width, height), (0, 0), 1))
    }

    /// Render the texture at the specified location.
    ///
    /// Using the `window_bounds` and `window_scale` parameters, it is possible to scale the
    /// texture and truncate it to be within the specified window bounds. The scaling will always
    /// take part **before** the truncation.
    pub fn draw_at(
        &self,
        frame: &mut Gles2Frame,
        output: &Output,
        window_bounds: Rectangle<i32, Logical>,
        window_scale: f64,
    ) {
        // Skip textures completely outside of the window bounds.
        let scaled_window_bounds = window_bounds.size.scale(1. / window_scale).max((1, 1));
        if self.location.x >= scaled_window_bounds.w || self.location.y >= scaled_window_bounds.h {
            return;
        }

        // Truncate source size based on window bounds.
        let src_size = (self.size + self.location).min(scaled_window_bounds);
        let src = Rectangle::from_loc_and_size((0, 0), src_size);

        // Scale output size based on window scale.
        let location = window_bounds.loc + self.location.scale(window_scale);
        let dest_size = src_size.scale(window_scale).min(window_bounds.size);
        let dest = Rectangle::from_loc_and_size(location, dest_size);

        let _ = frame.render_texture_from_to(
            &self.texture,
            src.to_buffer(self.scale),
            dest.to_f64().to_physical(output.scale),
            Transform::Normal,
            1.,
        );
    }

    /// Texture dimensions.
    pub fn size(&self) -> Size<i32, Logical> {
        self.size
    }
}

/// Grahpics texture cache.
#[derive(Debug)]
pub struct Graphics {
    pub active_drop_target: Texture,
    pub drop_target: Texture,
    decoration: Texture,
}

impl Graphics {
    pub fn new(renderer: &mut Gles2Renderer, output: &Output) -> Result<Self, Gles2Error> {
        Ok(Self {
            active_drop_target: Texture::from_buffer(renderer, &ACTIVE_DROP_TARGET_RGBA, 1, 1)?,
            drop_target: Texture::from_buffer(renderer, &DROP_TARGET_RGBA, 1, 1)?,
            decoration: Self::create_decoration(renderer, output)?,
        })
    }

    /// Get the window decoration texture corresponding to the active output size.
    pub fn decoration(&mut self, renderer: &mut Gles2Renderer, output: &Output) -> &Texture {
        if self.decoration.size != Self::decoration_size(output) {
            self.decoration = Self::create_decoration(renderer, output)
                .expect("decoration texture creation error");
        }

        &self.decoration
    }

    /// Decoration title bar height.
    pub fn title_height(output: &Output) -> i32 {
        (OVERVIEW_TITLE_HEIGHT as f64 * output.scale).round() as i32
    }

    /// Decoration border width.
    pub fn border_width(output: &Output) -> i32 {
        (OVERVIEW_BORDER_WIDTH as f64 * output.scale).round() as i32
    }

    /// Create overview window decoration.
    fn create_decoration(
        renderer: &mut Gles2Renderer,
        output: &Output,
    ) -> Result<Texture, Gles2Error> {
        let size = Self::decoration_size(output);
        let title_height = Self::title_height(output) as usize;
        let border_width = Self::border_width(output) as usize;

        let width = size.w as usize;
        let height = size.h as usize;

        let mut buffer = vec![0; width * height * 4];

        // Helper to fill rectangles of the buffer.
        let mut fill = |x_start, x_end, y_start, y_end, rgba: [u8; 4]| {
            for x in x_start..x_end {
                for y in y_start..y_end {
                    let start = y * width * 4 + x * 4;
                    buffer[start..start + 4].copy_from_slice(&rgba);
                }
            }
        };

        // Background.
        let right_border = width - border_width;
        let bottom_border = height - border_width;
        fill(border_width, right_border, title_height, bottom_border, BACKGROUND_RGBA);

        // Titlebar.
        fill(border_width, width, border_width, title_height - border_width, TITLE_RGBA);

        // Titlebar top border.
        fill(border_width, right_border, 0, border_width, BORDER_RGBA);

        // Titlebar bottom border.
        let title_border = title_height - border_width;
        fill(border_width, right_border, title_border, title_height, BORDER_RGBA);

        // Left border.
        fill(0, border_width, 0, height, BORDER_RGBA);

        // Right border.
        fill(right_border, width, 0, height, BORDER_RGBA);

        // Bottom border.
        fill(border_width, right_border, bottom_border, height, BORDER_RGBA);

        Texture::from_buffer(renderer, &buffer, size.w, size.h)
    }

    /// Total window decoration size.
    fn decoration_size(output: &Output) -> Size<i32, Logical> {
        let title_height = Self::title_height(output);
        let border_width = Self::border_width(output);

        let window_size = output.size().scale(FG_OVERVIEW_PERCENTAGE);
        let width = window_size.w + border_width * 2;
        let height = window_size.h + title_height + border_width;

        Size::from((width, height))
    }
}

/// Surface buffer cache.
pub struct SurfaceBuffer {
    pub texture: Option<Rc<Gles2Texture>>,
    pub buffer: Option<Buffer>,
    pub scale: i32,

    dimensions: Size<i32, Physical>,
}

impl Default for SurfaceBuffer {
    fn default() -> Self {
        Self {
            scale: 1,
            dimensions: Default::default(),
            texture: Default::default(),
            buffer: Default::default(),
        }
    }
}

impl SurfaceBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handle buffer creation/removal.
    pub fn update_buffer(&mut self, attributes: &SurfaceAttributes, assignment: BufferAssignment) {
        match assignment {
            BufferAssignment::NewBuffer { buffer, .. } => {
                self.dimensions = renderer::buffer_dimensions(&buffer).unwrap_or_default();
                self.scale = attributes.buffer_scale;
                self.buffer = Some(Buffer(buffer));
                self.texture = None;
            },
            BufferAssignment::Removed => *self = Self::default(),
        }
    }

    /// Surface size.
    pub fn size(&self) -> Size<i32, Logical> {
        self.dimensions.to_logical(self.scale)
    }
}

/// Container for automatically releasing a buffer on drop.
pub struct Buffer(WlBuffer);

impl Drop for Buffer {
    fn drop(&mut self) {
        self.0.release();
    }
}

impl Deref for Buffer {
    type Target = WlBuffer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
