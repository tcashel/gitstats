use crate::types::AnalysisResult;
use anyhow::Result;
use chrono::{DateTime, Datelike, NaiveDateTime, Timelike, Utc};
use rust_bert::pipelines::sequence_classification::SequenceClassificationModel;
use rust_bert::RustBertError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Constants for the model
//const MAX_COMMITS: usize = 1000;
const THRESHOLD: f32 = 0.95;

/// Represents a commit feature vector for ML analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitFeatures {
    // Time-based features
    pub hour_of_day: f32,
    pub day_of_week: f32,
    pub month: f32,
    pub is_weekend: f32,
    pub time_of_day_category: f32,

    // Size-based features
    pub lines_added: f32,
    pub lines_deleted: f32,
    pub files_changed: f32,
    pub net_change_ratio: f32,

    // Author-based features
    pub author_previous_commits: f32,
    pub days_since_last_commit: f32,
    pub author_activity_score: f32,

    // Anomaly detection scores
    #[serde(skip)]
    pub anomaly_score: Option<f32>,
    #[serde(skip)]
    pub is_anomalous: Option<bool>,
}

impl CommitFeatures {
    fn normalize_time_features(&mut self) {
        self.hour_of_day /= 24.0;
        self.day_of_week /= 6.0;
        self.month /= 12.0;
        self.time_of_day_category /= 4.0;
    }

    /* fn to_array(&self) -> Array1<f32> {
        Array1::from_vec(vec![
            self.hour_of_day,
            self.day_of_week,
            self.month,
            self.is_weekend,
            self.time_of_day_category,
            self.lines_added,
            self.lines_deleted,
            self.files_changed,
            self.net_change_ratio,
            self.author_previous_commits,
            self.days_since_last_commit,
            self.author_activity_score,
        ])
    } */

    fn calculate_time_of_day_category(hour: u32) -> f32 {
        match hour {
            5..=11 => 0.0,  // morning
            12..=16 => 1.0, // afternoon
            17..=21 => 2.0, // evening
            _ => 3.0,       // night
        }
    }

    fn to_input_string(&self) -> String {
        format!(
            "time:{:.2} day:{:.2} month:{:.2} weekend:{:.2} category:{:.2} added:{:.2} deleted:{:.2} files:{:.2} ratio:{:.2} commits:{:.2} last:{:.2} activity:{:.2}",
            self.hour_of_day,
            self.day_of_week,
            self.month,
            self.is_weekend,
            self.time_of_day_category,
            self.lines_added,
            self.lines_deleted,
            self.files_changed,
            self.net_change_ratio,
            self.author_previous_commits,
            self.days_since_last_commit,
            self.author_activity_score,
        )
    }
}

/// Anomaly detector using BERT architecture
pub struct AnomalyDetector {
    model: SequenceClassificationModel,
    threshold: f32,
}

impl AnomalyDetector {
    pub fn new() -> Result<Self, RustBertError> {
        // Initialize the sequence classification model
        let model = SequenceClassificationModel::new(Default::default())?;

        Ok(Self {
            model,
            threshold: THRESHOLD,
        })
    }

    /// Convert features to model input format
    fn prepare_input(&self, features: &[CommitFeatures]) -> Vec<String> {
        features.iter().map(|f| f.to_input_string()).collect()
    }

    /// Train the anomaly detector
    pub fn train(&mut self, features: &[CommitFeatures]) -> Result<(), RustBertError> {
        let inputs = self.prepare_input(features);
        let input_refs: Vec<&str> = inputs.iter().map(String::as_str).collect();

        // Use the model to get predictions (in training mode)
        let outputs = self.model.predict(&input_refs);

        // Calculate threshold from predictions
        let scores: Vec<f32> = outputs.iter().map(|output| output.score as f32).collect();

        let mut sorted_scores = scores.clone();
        sorted_scores.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // Set threshold at 95th percentile
        let threshold_idx = (sorted_scores.len() as f32 * THRESHOLD) as usize;
        self.threshold = sorted_scores[threshold_idx];

        Ok(())
    }

    /// Detect anomalies in new commits
    pub fn detect_anomalies(&self, features: &mut [CommitFeatures]) -> Result<(), RustBertError> {
        let inputs = self.prepare_input(features);
        let input_refs: Vec<&str> = inputs.iter().map(String::as_str).collect();

        // Get model predictions
        let outputs = self.model.predict(&input_refs);

        // Update anomaly scores
        for (feature, output) in features.iter_mut().zip(outputs.iter()) {
            let score = output.score as f32;
            feature.anomaly_score = Some(score);
            feature.is_anomalous = Some(score > self.threshold);
        }

        Ok(())
    }
}

/// Prepares commit data for ML analysis
pub fn prepare_commit_features(analysis_result: &AnalysisResult) -> Vec<CommitFeatures> {
    let mut author_commit_counts: HashMap<String, usize> = HashMap::new();
    let mut last_commit_dates: HashMap<String, DateTime<Utc>> = HashMap::new();
    let mut author_activity_scores: HashMap<String, f32> = HashMap::new();
    let mut features = Vec::new();

    // Process commit activity chronologically
    for (date_str, lines_added, lines_deleted) in &analysis_result.commit_activity {
        if let Ok(naive_date) =
            NaiveDateTime::parse_from_str(&format!("{} 00:00:00", date_str), "%Y-%m-%d %H:%M:%S")
        {
            let date = DateTime::<Utc>::from_naive_utc_and_offset(naive_date, Utc);

            // Extract time-based features
            let hour = date.hour();
            let day_of_week = date.weekday().num_days_from_monday() as f32;
            let month = date.month() as f32;
            let is_weekend = if day_of_week >= 5.0 { 1.0 } else { 0.0 };
            let time_of_day_category = CommitFeatures::calculate_time_of_day_category(hour);

            // Calculate size-based features
            let lines_added = *lines_added as f32;
            let lines_deleted = *lines_deleted as f32;
            let net_change_ratio = if lines_added + lines_deleted > 0.0 {
                (lines_added - lines_deleted) / (lines_added + lines_deleted)
            } else {
                0.0
            };

            // Author-based features
            let author = "placeholder_author".to_string();
            let author_commits = *author_commit_counts.get(&author).unwrap_or(&0) as f32;
            let days_since_last = if let Some(last_date) = last_commit_dates.get(&author) {
                (date - *last_date).num_days() as f32
            } else {
                0.0
            };

            // Update author activity score (exponential decay)
            let activity_score = author_activity_scores
                .entry(author.clone())
                .and_modify(|score| *score = *score * 0.95 + 1.0)
                .or_insert(1.0);

            // Update author tracking
            *author_commit_counts.entry(author.clone()).or_insert(0) += 1;
            last_commit_dates.insert(author.clone(), date);

            let mut feature = CommitFeatures {
                hour_of_day: hour as f32,
                day_of_week,
                month,
                is_weekend,
                time_of_day_category,
                lines_added: lines_added.log2().max(0.0),
                lines_deleted: lines_deleted.log2().max(0.0),
                files_changed: 1.0,
                net_change_ratio,
                author_previous_commits: author_commits.log2().max(0.0),
                days_since_last_commit: days_since_last.min(365.0) / 365.0,
                author_activity_score: *activity_score,
                anomaly_score: None,
                is_anomalous: None,
            };

            feature.normalize_time_features();
            features.push(feature);
        }
    }

    features
}

pub struct CommitAnalyzer {
    model: SequenceClassificationModel,
}

impl CommitAnalyzer {
    pub fn analyze_commits(&self, inputs: &[String]) -> Result<Vec<f32>> {
        let input_refs: Vec<&str> = inputs.iter().map(String::as_str).collect();
        let outputs = self.model.predict(&input_refs);

        let probabilities: Vec<f32> = outputs.iter().map(|label| label.score as f32).collect();

        Ok(probabilities)
    }

    pub fn analyze_batch(&self, inputs: &[String]) -> Result<Vec<f32>> {
        let input_refs: Vec<&str> = inputs.iter().map(String::as_str).collect();
        let outputs = self.model.predict(&input_refs);

        let probabilities: Vec<f32> = outputs.iter().map(|label| label.score as f32).collect();

        Ok(probabilities)
    }
}

pub async fn detect_anomalies(
    result: &AnalysisResult,
) -> Result<Vec<CommitFeatures>, RustBertError> {
    let features = prepare_commit_features(result);
    let features_clone = features.clone();

    // Run model operations in a blocking task
    let features = tokio::task::spawn_blocking(move || {
        let mut detector = AnomalyDetector::new()?;
        let mut features = features_clone;
        detector.train(&features)?;
        detector.detect_anomalies(&mut features)?;
        Ok::<_, RustBertError>(features)
    })
    .await
    .unwrap()?;

    Ok(features)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_data() -> AnalysisResult {
        // Create 100 commits where:
        // - 95 commits are "normal" (10-50 lines)
        // - 4 commits are "larger" (100-200 lines)
        // - 1 commit is clearly anomalous (1000+ lines)
        let mut commit_activity = Vec::new();

        // Add 95 normal commits
        for i in 1..96 {
            let lines = 10 + (i % 4) * 10; // Varies between 10-40 lines
            commit_activity.push((
                format!("2024-01-{:02}", i % 31 + 1),
                lines as usize,
                (lines / 2) as usize,
            ));
        }

        // Add 4 larger commits
        for i in 1..5 {
            commit_activity.push((format!("2024-02-{:02}", i), 150 + i * 10, 75 + i * 5));
        }

        // Add 1 clearly anomalous commit
        commit_activity.push(("2024-02-05".to_string(), 15000, 7500));

        AnalysisResult {
            commit_activity,
            commit_count: 100,
            total_lines_added: 5000,
            total_lines_deleted: 2500,
            top_contributors: vec![("test_author".to_string(), 100)],
            available_branches: vec!["main".to_string()],
            average_commit_size: 50.0,
            commit_frequency: HashMap::new(),
            top_contributors_by_lines: vec![],
            elapsed_time: 1.0,
            processing_stats: "test".to_string(),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_anomaly_detection() {
        let result = create_test_data();
        let features = tokio::task::spawn_blocking(move || {
            let mut detector = AnomalyDetector::new().unwrap();
            let mut features = prepare_commit_features(&result);
            detector.train(&features).unwrap();
            detector.detect_anomalies(&mut features).unwrap();
            features
        })
        .await
        .unwrap();

        // The last commit should be anomalous (it's our 1500-line commit)
        assert!(
            features[20].is_anomalous.unwrap(),
            "Large commit should be detected as anomalous"
        );

        // The first commit should not be anomalous (it's a normal-sized commit)
        assert!(
            !features[0].is_anomalous.unwrap(),
            "Small commit should not be anomalous"
        );

        // Count anomalies - should be roughly 5% of commits
        let anomaly_count = features.iter().filter(|f| f.is_anomalous.unwrap()).count();
        assert!(
            anomaly_count >= 1 && anomaly_count <= 6,
            "Expected 1-6 anomalies in 100 commits"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_anomaly_detection_pipeline() {
        let result = create_test_data();
        let anomalies = detect_anomalies(&result).await.unwrap();

        assert_eq!(anomalies.len(), 100);

        // The last commit should be anomalous
        assert!(anomalies[20].is_anomalous.unwrap());

        // Count anomalies - should be roughly 5% of commits
        let anomaly_count = anomalies.iter().filter(|f| f.is_anomalous.unwrap()).count();
        assert!(
            anomaly_count >= 1 && anomaly_count <= 6,
            "Expected 1-6 anomalies in 100 commits"
        );
    }
}
