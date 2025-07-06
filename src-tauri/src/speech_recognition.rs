use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use log::{info, error, warn};
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};
use crate::TranscriptionResult;

pub struct SpeechRecognizer {
    whisper_context: Option<Arc<WhisperContext>>,
    is_initialized: bool,
    sample_rate: i32,
}

impl SpeechRecognizer {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        info!("Initializing Speech Recognition system...");
        
        Ok(Self {
            whisper_context: None,
            is_initialized: false,
            sample_rate: 16000, // Whisper expects 16kHz
        })
    }

    pub fn initialize(&mut self, model_path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_initialized {
            return Ok(());
        }

        info!("Loading Whisper model...");
        
        // Use default model or provided path
        let default_model = "models/ggml-base.en.bin";
        let model_path = model_path.unwrap_or(default_model);
        
        // Try multiple possible locations for the model
        let possible_paths = vec![
            "ggml-base.en.bin".to_string(), // First try local to binary
            model_path.to_string(),
            format!("../{}", model_path),
            format!("../../{}", model_path),
            format!("/Users/thiago/Projects/devcaption/{}", model_path),
            format!("{}/models/ggml-base.en.bin", std::env::current_dir()?.parent().unwrap_or(std::env::current_dir()?.as_ref()).display()),
        ];
        
        let mut found_path = None;
        for path in &possible_paths {
            info!("Checking model path: {}", path);
            if std::path::Path::new(path).exists() {
                found_path = Some(path.clone());
                break;
            }
        }
        
        let final_model_path = found_path.ok_or_else(|| {
            let cwd = std::env::current_dir().unwrap_or_default();
            let error_msg = format!(
                "Whisper model not found. Tried paths: {:?}. Current working directory: {}",
                possible_paths, cwd.display()
            );
            warn!("{}", error_msg);
            error_msg
        })?;

        let ctx_params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(&final_model_path, ctx_params)
            .map_err(|e| format!("Failed to load Whisper model from {}: {}", final_model_path, e))?;

        self.whisper_context = Some(Arc::new(ctx));
        self.is_initialized = true;
        
        info!("Whisper model loaded successfully from: {}", final_model_path);
        Ok(())
    }

    pub fn transcribe_audio(&self, audio_data: &[f32]) -> Result<TranscriptionResult, Box<dyn std::error::Error>> {
        if !self.is_initialized {
            return Err("Speech recognizer not initialized".into());
        }

        let ctx = self.whisper_context.as_ref()
            .ok_or("Whisper context not available")?;

        info!("Starting transcription of {} samples", audio_data.len());

        // Audio should already be mono and at 16kHz from the capture system
        let processed_audio = audio_data.to_vec();

        // Set up parameters for transcription
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(4);
        params.set_translate(false);
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        params.set_single_segment(true);

        // Run inference
        let mut state = ctx.create_state()?;
        state.full(params, &processed_audio)?;

        // Get the transcribed text
        let num_segments = state.full_n_segments()?;
        let mut text = String::new();
        let mut total_confidence = 0.0;
        let mut token_count = 0;

        for segment_index in 0..num_segments {
            let segment_text = state.full_get_segment_text(segment_index)?;
            text.push_str(&segment_text);
            
            let num_tokens = state.full_n_tokens(segment_index)?;
            for token_index in 0..num_tokens {
                let token_prob = state.full_get_token_prob(segment_index, token_index)?;
                total_confidence += token_prob;
                token_count += 1;
            }
        }

        let confidence = if token_count > 0 {
            total_confidence / token_count as f32
        } else {
            0.0
        };

        let result = TranscriptionResult {
            text: text.trim().to_string(),
            confidence: confidence as f64,
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
            is_final: true,
        };

        info!("Transcription completed: '{}' (confidence: {:.2})", result.text, result.confidence);

        Ok(result)
    }

    fn preprocess_audio(&self, audio_data: &[f32]) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        // For now, assume the input is already at the correct sample rate
        // In a full implementation, we'd resample from 48kHz to 16kHz
        
        // Convert stereo to mono if needed (simple average)
        if audio_data.len() % 2 == 0 {
            let mono_data: Vec<f32> = audio_data
                .chunks_exact(2)
                .map(|chunk| (chunk[0] + chunk[1]) / 2.0)
                .collect();
            Ok(mono_data)
        } else {
            Ok(audio_data.to_vec())
        }
    }

    pub fn is_ready(&self) -> bool {
        self.is_initialized && self.whisper_context.is_some()
    }
}

impl Default for SpeechRecognizer {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| {
            error!("Failed to create default SpeechRecognizer: {}", e);
            Self {
                whisper_context: None,
                is_initialized: false,
                sample_rate: 16000,
            }
        })
    }
}

// Utility functions for audio processing
pub fn detect_voice_activity(audio_data: &[f32], threshold: f32) -> bool {
    if audio_data.is_empty() {
        return false;
    }

    // Calculate RMS energy
    let rms = (audio_data.iter().map(|&x| x * x).sum::<f32>() / audio_data.len() as f32).sqrt();
    
    info!("Voice activity detection: RMS = {:.4}, threshold = {:.4}", rms, threshold);
    
    rms > threshold
}

pub fn apply_noise_reduction(audio_data: &mut [f32], noise_level: f32) {
    // Simple noise gate
    for sample in audio_data.iter_mut() {
        if sample.abs() < noise_level {
            *sample = 0.0;
        }
    }
}