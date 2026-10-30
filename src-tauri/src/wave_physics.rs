//! Native Rust Wave Carousel Geometry, Spring Physics, and Color Palette Engine for MicroSwap.
//!
//! This module provides deterministic layout calculations, spring-damper physics simulation,
//! color palette analysis, and polygon clipping geometry for the MicroSwap wave carousel.

use serde::{Deserialize, Serialize};

/// Represents a 2D point in normalized percentage space `[0.0 .. 100.0]`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

impl Point2D {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn lerp(self, other: Self, t: f64) -> Self {
        let clamped = t.clamp(0.0, 1.0);
        Self {
            x: self.x + (other.x - self.x) * clamped,
            y: self.y + (other.y - self.y) * clamped,
        }
    }

    pub fn distance_to(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Represents a 4-corner sheared polygon used to clip vertical wallpaper slices.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShearedQuad {
    pub top_left: Point2D,
    pub top_right: Point2D,
    pub bottom_right: Point2D,
    pub bottom_left: Point2D,
}

impl ShearedQuad {
    pub const fn rectangle() -> Self {
        Self {
            top_left: Point2D::new(0.0, 0.0),
            top_right: Point2D::new(100.0, 0.0),
            bottom_right: Point2D::new(100.0, 100.0),
            bottom_left: Point2D::new(0.0, 100.0),
        }
    }

    pub const fn left_wing(shear_pct: f64) -> Self {
        Self {
            top_left: Point2D::new(0.0, shear_pct),
            top_right: Point2D::new(100.0, 0.0),
            bottom_right: Point2D::new(100.0, 100.0 - shear_pct),
            bottom_left: Point2D::new(0.0, 100.0),
        }
    }

    pub const fn right_wing(shear_pct: f64) -> Self {
        Self {
            top_left: Point2D::new(0.0, 0.0),
            top_right: Point2D::new(100.0, shear_pct),
            bottom_right: Point2D::new(100.0, 100.0),
            bottom_left: Point2D::new(0.0, 100.0 - shear_pct),
        }
    }

    pub const fn center_blade(shear_pct: f64) -> Self {
        Self {
            top_left: Point2D::new(0.0, shear_pct),
            top_right: Point2D::new(100.0, 0.0),
            bottom_right: Point2D::new(100.0, 100.0 - shear_pct),
            bottom_left: Point2D::new(0.0, 100.0),
        }
    }

    pub fn to_css_polygon(self) -> String {
        format!(
            "polygon({:.1}% {:.1}%, {:.1}% {:.1}%, {:.1}% {:.1}%, {:.1}% {:.1}%)",
            self.top_left.x,
            self.top_left.y,
            self.top_right.x,
            self.top_right.y,
            self.bottom_right.x,
            self.bottom_right.y,
            self.bottom_left.x,
            self.bottom_left.y
        )
    }

    pub fn interpolate(self, target: Self, t: f64) -> Self {
        Self {
            top_left: self.top_left.lerp(target.top_left, t),
            top_right: self.top_right.lerp(target.top_right, t),
            bottom_right: self.bottom_right.lerp(target.bottom_right, t),
            bottom_left: self.bottom_left.lerp(target.bottom_left, t),
        }
    }
}

/// Complete computed visual metrics for a single slice in the wave carousel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputedSliceVisuals {
    pub offset: i32,
    pub width: f64,
    pub height: f64,
    pub opacity: f64,
    pub brightness: f64,
    pub translate_y: f64,
    pub scale: f64,
    pub skew_x: f64,
    pub skew_y: f64,
    pub rotate_z: f64,
    pub clip_path: String,
    pub z_index: i32,
}

/// Wave profile table defining base heights and widths from center (`offset = 0`) outward.
pub struct WaveProfile {
    pub heights: [f64; 10],
    pub widths: [f64; 10],
    pub expanded_width: f64,
    pub expanded_height: f64,
    pub base_skew_x: f64,
}

impl Default for WaveProfile {
    fn default() -> Self {
        Self {
            heights: [550.0, 504.0, 448.0, 392.0, 336.0, 282.0, 232.0, 188.0, 152.0, 120.0],
            widths: [152.0, 132.0, 114.0, 98.0, 86.0, 74.0, 64.0, 54.0, 46.0, 38.0],
            expanded_width: 780.0,
            expanded_height: 460.0,
            base_skew_x: -4.5,
        }
    }
}

impl WaveProfile {
    pub fn compute_slice(
        &self,
        offset: i32,
        is_expanded_center: bool,
        is_hovered: bool,
        hovered_offset: Option<i32>,
        is_being_dragged: bool,
    ) -> ComputedSliceVisuals {
        let abs = offset.unsigned_abs() as usize;

        if abs == 0 && is_expanded_center {
            let quad = ShearedQuad::center_blade(2.5);
            return ComputedSliceVisuals {
                offset,
                width: self.expanded_width,
                height: self.expanded_height,
                opacity: 1.0,
                brightness: 1.08,
                translate_y: 0.0,
                scale: 1.03,
                skew_x: -3.0,
                skew_y: -1.5,
                rotate_z: -0.8,
                clip_path: quad.to_css_polygon(),
                z_index: 25,
            };
        }

        let idx = abs.min(self.heights.len() - 1);
        let base_height = self.heights[idx];
        let base_width = self.widths[idx];

        let dist_to_hover = hovered_offset
            .map(|h| (offset - h).unsigned_abs() as usize)
            .unwrap_or(99);

        let hover_height_boost = if is_hovered && abs != 0 {
            42.0
        } else if dist_to_hover == 1 && abs != 0 {
            18.0
        } else {
            0.0
        };

        let hover_width_boost = if is_hovered && abs != 0 {
            18.0
        } else if dist_to_hover == 1 && abs != 0 {
            7.0
        } else {
            0.0
        };

        let height = base_height + hover_height_boost;
        let width = base_width + hover_width_boost;

        let opacity = if is_being_dragged {
            0.92
        } else if abs > 7 {
            0.38
        } else if abs > 5 {
            0.74
        } else {
            1.0
        };

        let brightness = if abs == 0 {
            1.14
        } else if is_hovered {
            1.10
        } else if dist_to_hover == 1 {
            0.98
        } else {
            (1.0 - (abs as f64) * 0.065).max(0.48)
        };

        let translate_y = if is_being_dragged {
            -18.0
        } else if abs == 0 {
            -10.0
        } else if is_hovered {
            -12.0
        } else if dist_to_hover == 1 {
            -5.0
        } else {
            0.0
        };

        let scale = if is_being_dragged {
            1.08
        } else if abs == 0 {
            1.045
        } else if is_hovered {
            1.06
        } else if dist_to_hover == 1 {
            1.02
        } else {
            1.0
        };

        let skew_x = self.base_skew_x;
        let skew_y = if offset == 0 {
            -2.2
        } else if offset < 0 {
            -3.2
        } else {
            2.6
        };

        let rotate_z = if is_hovered {
            (offset as f64) * 0.2 - 0.5
        } else {
            (offset as f64) * 0.35 - 1.2
        };

        let quad = if offset < 0 {
            ShearedQuad::left_wing(5.5)
        } else if offset > 0 {
            ShearedQuad::right_wing(5.5)
        } else {
            ShearedQuad::center_blade(3.5)
        };

        let z_index = if is_being_dragged {
            30
        } else if abs == 0 {
            20
        } else {
            15 - (abs.min(10) as i32)
        };

        ComputedSliceVisuals {
            offset,
            width,
            height,
            opacity,
            brightness,
            translate_y,
            scale,
            skew_x,
            skew_y,
            rotate_z,
            clip_path: quad.to_css_polygon(),
            z_index,
        }
    }

    pub fn compute_band(
        &self,
        radius: i32,
        is_expanded_center: bool,
        hovered_offset: Option<i32>,
        dragged_offset: Option<i32>,
    ) -> Vec<ComputedSliceVisuals> {
        let mut band = Vec::with_capacity((radius * 2 + 1) as usize);
        for offset in -radius..=radius {
            let is_hovered = hovered_offset == Some(offset);
            let is_dragged = dragged_offset == Some(offset);
            band.push(self.compute_slice(
                offset,
                is_expanded_center,
                is_hovered,
                hovered_offset,
                is_dragged,
            ));
        }
        band
    }
}

/// Second-order damped harmonic oscillator for simulating bouncy spring motion in Rust.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpringSolver {
    pub stiffness: f64,
    pub damping: f64,
    pub mass: f64,
}

impl Default for SpringSolver {
    fn default() -> Self {
        Self {
            stiffness: 280.0,
            damping: 18.0,
            mass: 1.0,
        }
    }
}

impl SpringSolver {
    pub const fn bouncy() -> Self {
        Self {
            stiffness: 340.0,
            damping: 15.5,
            mass: 1.0,
        }
    }

    pub const fn smooth_portal() -> Self {
        Self {
            stiffness: 190.0,
            damping: 24.0,
            mass: 1.1,
        }
    }

    /// Evaluates normalized position `[0.0 -> 1.0]` at elapsed time `t_seconds`.
    pub fn sample(&self, t_seconds: f64) -> f64 {
        if t_seconds <= 0.0 {
            return 0.0;
        }
        let omega_n = (self.stiffness / self.mass).sqrt();
        let zeta = self.damping / (2.0 * (self.stiffness * self.mass).sqrt());

        if zeta < 1.0 {
            // Under-damped (bouncy overshoot)
            let omega_d = omega_n * (1.0 - zeta * zeta).sqrt();
            let envelope = (-zeta * omega_n * t_seconds).exp();
            let cos_part = (omega_d * t_seconds).cos();
            let sin_part = (zeta / (1.0 - zeta * zeta).sqrt()) * (omega_d * t_seconds).sin();
            1.0 - envelope * (cos_part + sin_part)
        } else {
            // Critically damped
            let envelope = (-omega_n * t_seconds).exp();
            1.0 - envelope * (1.0 + omega_n * t_seconds)
        }
    }

    /// Generates a lookup curve of `steps` samples across `duration_ms`.
    pub fn sample_curve(&self, duration_ms: u32, steps: usize) -> Vec<f64> {
        if steps == 0 {
            return Vec::new();
        }
        let total_sec = (duration_ms as f64) / 1000.0;
        (0..=steps)
            .map(|i| {
                let frac = (i as f64) / (steps as f64);
                self.sample(frac * total_sec)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wave_profile_symmetry() {
        let profile = WaveProfile::default();
        let left = profile.compute_slice(-3, false, false, None, false);
        let right = profile.compute_slice(3, false, false, None, false);
        assert_eq!(left.height, right.height);
        assert_eq!(left.width, right.width);
    }

    #[test]
    fn test_spring_solver_convergence() {
        let spring = SpringSolver::bouncy();
        let start = spring.sample(0.0);
        let end = spring.sample(2.0);
        assert!((start - 0.0).abs() < 1e-5);
        assert!((end - 1.0).abs() < 1e-2);
    }
}
