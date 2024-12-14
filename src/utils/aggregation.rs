/// Aggregate data points to reduce visual noise and improve performance
pub fn aggregate_data(data: &[(String, usize, usize)], target_points: usize) -> Vec<(String, usize, usize)> {
    if data.len() <= target_points {
        return data.to_vec();
    }

    let window_size = (data.len() as f64 / target_points as f64).ceil() as usize;
    let mut aggregated = Vec::new();

    for chunk in data.chunks(window_size) {
        let date = chunk[0].0.clone(); // Use first date in chunk
        let total_added: usize = chunk.iter().map(|(_, added, _)| *added).sum();
        let total_deleted: usize = chunk.iter().map(|(_, _, deleted)| *deleted).sum();
        aggregated.push((date, total_added, total_deleted));
    }

    aggregated
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_no_aggregation_needed() {
        let data = vec![
            ("2023-01-01".to_string(), 10, 5),
            ("2023-01-02".to_string(), 20, 10),
        ];
        let target_points = 5;

        let result = aggregate_data(&data, target_points);
        assert_eq!(result, data);
    }

    #[test]
    fn test_basic_aggregation() {
        let data = vec![
            ("2023-01-01".to_string(), 10, 5),
            ("2023-01-02".to_string(), 20, 10),
            ("2023-01-03".to_string(), 30, 15),
            ("2023-01-04".to_string(), 40, 20),
        ];
        let target_points = 2;

        let result = aggregate_data(&data, target_points);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("2023-01-01".to_string(), 30, 15));
        assert_eq!(result[1], ("2023-01-03".to_string(), 70, 35));
    }

    #[test]
    fn test_empty_data() {
        let data: Vec<(String, usize, usize)> = vec![];
        let target_points = 5;

        let result = aggregate_data(&data, target_points);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_point() {
        let data = vec![("2023-01-01".to_string(), 10, 5)];
        let target_points = 5;

        let result = aggregate_data(&data, target_points);
        assert_eq!(result, data);
    }

    #[test]
    fn test_uneven_chunks() {
        let data = vec![
            ("2023-01-01".to_string(), 10, 5),
            ("2023-01-02".to_string(), 20, 10),
            ("2023-01-03".to_string(), 30, 15),
            ("2023-01-04".to_string(), 40, 20),
            ("2023-01-05".to_string(), 50, 25),
        ];
        let target_points = 2;

        let result = aggregate_data(&data, target_points);
        // With 5 points and target of 2, we get a window size of 3 (ceil(5/2)),
        // resulting in two chunks: [0,1,2] and [3,4]
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("2023-01-01".to_string(), 60, 30)); // Sum of first 3 points
        assert_eq!(result[1], ("2023-01-04".to_string(), 90, 45)); // Sum of last 2 points
    }
} 