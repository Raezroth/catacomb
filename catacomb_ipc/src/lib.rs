//! Catacomb compositor interface.
//!
//! This library provides abstractions for interacting with Catacomb's external
//! interfaces from Wayland clients.

//! IPC socket communication.

use std::error::Error;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::str::FromStr;
use std::{env, process};

#[cfg(feature = "clap")]
use clap::Subcommand;
use serde::{Deserialize, Serialize};
#[cfg(feature = "smithay")]
use smithay::utils::{Logical, Point, Size, Transform};

/// IPC message format.
#[cfg_attr(feature = "clap", derive(Subcommand))]
#[derive(Deserialize, Serialize, Debug)]
pub enum IpcMessage {
    /// Screen rotation (un)locking.
    Orientation {
        /// Lock rotation in the specified orientation.
        #[cfg_attr(feature = "clap", clap(long, min_values = 0, conflicts_with = "unlock"))]
        lock: Option<Orientation>,
        /// Clear screen rotation lock.
        #[cfg_attr(feature = "clap", clap(long))]
        unlock: bool,
    },
    /// Update output scale factor.
    Scale {
        /// New scale factor.
        scale: f64,
    },
    /// Add a gesture.
    Bind {
        /// App ID regex for which the gesture.
        app_id: String,
        /// Starting sector of the gesture.
        start: GestureSector,
        /// Termination sector of the gesture.
        end: GestureSector,
        /// Programm this gesture should spawn.
        program: String,
        /// Arguments for this gesture's program.
        #[cfg_attr(feature = "clap", clap(allow_hyphen_values = true, multiple_values = true))]
        arguments: Vec<String>,
    },
    /// Remove a gesture.
    Unbind {
        /// App ID regex of the gesture.
        app_id: String,
        /// Starting sector of the gesture.
        start: GestureSector,
        /// Termination sector of the gesture.
        end: GestureSector,
    },
}

/// Device orientation.
#[derive(Deserialize, Serialize, Default, PartialEq, Eq, Copy, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Orientation {
    /// Portrait mode.
    #[default]
    Portrait,

    // Inverse portrait mode.
    InversePortrait,

    /// Landscape mode.
    Landscape,

    /// Inverse landscape mode.
    InverseLandscape,
}

impl FromStr for Orientation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "portrait" => Ok(Self::Portrait),
            "inverse-portrait" => Ok(Self::InversePortrait),
            "landscape" => Ok(Self::Landscape),
            "inverse-landscape" => Ok(Self::InverseLandscape),
            _ => Err(format!(
                "Got {s:?}, expected one of portrait, inverse-portrait, landscape, or \
                 inverse-landscape"
            )),
        }
    }
}

#[cfg(feature = "smithay")]
impl Orientation {
    /// Output rendering transform for this orientation.
    #[must_use]
    pub fn output_transform(&self) -> Transform {
        match self {
            Self::Portrait => Transform::Normal,
            Self::InversePortrait => Transform::_180,
            Self::Landscape => Transform::_90,
            Self::InverseLandscape => Transform::_270,
        }
    }

    /// Surface rendering transform for this orientation.
    #[must_use]
    pub fn surface_transform(&self) -> Transform {
        match self {
            Self::Portrait => Transform::Normal,
            Self::InversePortrait => Transform::_180,
            Self::Landscape => Transform::_270,
            Self::InverseLandscape => Transform::_90,
        }
    }
}

/// Gesture start/end sectors.
#[derive(Deserialize, Serialize, PartialEq, Eq, Copy, Clone, Debug)]
pub enum GestureSector {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    MiddleCenter,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl FromStr for GestureSector {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "topleft" | "tl" => Ok(Self::TopLeft),
            "topcenter" | "tc" => Ok(Self::TopCenter),
            "topright" | "tr" => Ok(Self::TopRight),
            "middleleft" | "ml" => Ok(Self::MiddleLeft),
            "middlecenter" | "mc" => Ok(Self::MiddleCenter),
            "middleright" | "mr" => Ok(Self::MiddleRight),
            "bottomleft" | "bl" => Ok(Self::BottomLeft),
            "bottomcenter" | "bc" => Ok(Self::BottomCenter),
            "bottomright" | "br" => Ok(Self::BottomRight),
            _ => Err(format!(
                "Got {s:?}, expected one of topleft, topcenter, topright, middleleft, \
                 middlecenter, middleright, bottomleft, bottomcenter, or bottomright"
            )),
        }
    }
}

impl GestureSector {
    /// Get output sector a point lies in.
    #[cfg(feature = "smithay")]
    pub fn from_point(output_size: Size<f64, Logical>, point: Point<f64, Logical>) -> Self {
        // Map point in the range of 0..=2 for X and Y.
        let x_mult = (point.x / (output_size.w / 3.)).floor() as u32;
        let y_mult = (point.y / (output_size.h / 3.)).floor() as u32;

        match (x_mult.min(2), y_mult.min(2)) {
            (0, 0) => Self::TopLeft,
            (1, 0) => Self::TopCenter,
            (2, 0) => Self::TopRight,
            (0, 1) => Self::MiddleLeft,
            (1, 1) => Self::MiddleCenter,
            (2, 1) => Self::MiddleRight,
            (0, 2) => Self::BottomLeft,
            (1, 2) => Self::BottomCenter,
            (2, 2) => Self::BottomRight,
            _ => unreachable!(),
        }
    }
}

/// Send a message to the Catacomb IPC socket.
pub fn send_message(message: &IpcMessage) -> Result<(), Box<dyn Error>> {
    let socket_name = match env::var("WAYLAND_DISPLAY") {
        Ok(socket_name) => socket_name,
        Err(_) => {
            eprintln!("Error: WAYLAND_DISPLAY is not set");
            process::exit(101);
        },
    };

    let socket_path = socket_path(&socket_name);

    // Ensure Catacomb's IPC listener is running.
    if !socket_path.exists() {
        eprintln!("Error: IPC socket not found, ensure Catacomb is running");
        process::exit(102);
    }

    let mut socket = UnixStream::connect(&socket_path)?;

    let message = serde_json::to_string(&message)?;
    socket.write_all(message[..].as_bytes())?;
    let _ = socket.flush();

    Ok(())
}

/// Path for the IPC socket file.
pub fn socket_path(socket_name: &str) -> PathBuf {
    dirs::runtime_dir().unwrap_or_else(env::temp_dir).join(format!("catacomb-{socket_name}.sock"))
}
