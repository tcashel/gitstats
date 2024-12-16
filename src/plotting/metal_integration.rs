use super::metal_renderer::{MetalRenderer, Vertex};
use std::error::Error;

pub type PlotResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct GpuPlotter {
    renderer: MetalRenderer,
    width: u32,
    height: u32,
}

impl GpuPlotter {
    pub fn new(width: u32, height: u32) -> PlotResult<Self> {
        Ok(Self {
            renderer: MetalRenderer::new()?,
            width,
            height,
        })
    }

    pub fn plot_line_series(&self, points: &[(f64, f64)], color: [f32; 4]) -> PlotResult<Vec<u8>> {
        // Convert points to vertices
        let vertices: Vec<Vertex> = points
            .iter()
            .map(|(x, y)| Vertex {
                position: [*x as f32, *y as f32],
                color,
            })
            .collect();

        // Render to texture
        Ok(self.renderer.render_to_texture(&vertices, self.width, self.height)?)
    }

    pub fn plot_bars(&self, bars: &[(f64, f64)], color: [f32; 4]) -> PlotResult<Vec<u8>> {
        // Convert bars to triangles (two triangles per bar)
        let mut vertices = Vec::with_capacity(bars.len() * 6);
        
        for &(x, height) in bars {
            let x0 = x as f32;
            let x1 = (x + 0.8) as f32; // Bar width
            let y0 = 0.0;
            let y1 = height as f32;

            // First triangle
            vertices.push(Vertex {
                position: [x0, y0],
                color,
            });
            vertices.push(Vertex {
                position: [x1, y0],
                color,
            });
            vertices.push(Vertex {
                position: [x0, y1],
                color,
            });

            // Second triangle
            vertices.push(Vertex {
                position: [x1, y0],
                color,
            });
            vertices.push(Vertex {
                position: [x1, y1],
                color,
            });
            vertices.push(Vertex {
                position: [x0, y1],
                color,
            });
        }

        // Render to texture
        Ok(self.renderer.render_to_texture(&vertices, self.width, self.height)?)
    }
} 