/// Chart theme configuration
pub struct ChartTheme {
    pub background_color: plotters::style::RGBAColor,
    pub text_color: plotters::style::RGBAColor,
    pub grid_color: plotters::style::RGBAColor,
    pub axis_color: plotters::style::RGBAColor,
}

impl Default for ChartTheme {
    fn default() -> Self {
        Self {
            background_color: plotters::style::RGBAColor(0, 0, 0, 0.94),
            text_color: plotters::style::RGBAColor(255, 255, 255, 0.8),
            grid_color: plotters::style::RGBAColor(255, 255, 255, 0.15),
            axis_color: plotters::style::RGBAColor(255, 255, 255, 0.8),
        }
    }
}

/// Chart style configuration
pub struct ChartStyle {
    pub line_width: u32,
    pub font_size: u32,
    pub margin: u32,
    pub label_area_size: u32,
}

impl Default for ChartStyle {
    fn default() -> Self {
        Self {
            line_width: 2,
            font_size: 15,
            margin: 10,
            label_area_size: 50,
        }
    }
} 