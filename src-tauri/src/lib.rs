use std::sync::{Arc, Mutex};
use std::thread;
use serde::{Deserialize, Serialize};
use log::{info, error};
use tauri::Emitter;
use serde_json;

mod audio_capture;
mod speech_recognition;
mod system_audio;
mod gemini_service;

use audio_capture::AudioCaptureSystem;
use speech_recognition::SpeechRecognizer;
use system_audio::SystemAudioHelper;
use gemini_service::GeminiService;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub confidence: f64,
    pub timestamp: u64,
    pub is_final: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioLevel {
    pub level: f64,
    pub timestamp: u64,
}

// Global state for audio capture and speech recognition
static CAPTURE_SYSTEM: Mutex<Option<Arc<AudioCaptureSystem>>> = Mutex::new(None);
static SPEECH_RECOGNIZER: Mutex<Option<Arc<Mutex<SpeechRecognizer>>>> = Mutex::new(None);

// Constants
const GEMINI_API_KEY: &str = "AIzaSyBzcVnMVBRXHGWbAhAaSQdoubc6YuLkcv8";

#[tauri::command]
async fn start_audio_capture(window: tauri::Window, device_name: Option<String>) -> Result<String, String> {
    info!("Starting audio capture...");
    
    let mut capture_system = CAPTURE_SYSTEM.lock().map_err(|e| e.to_string())?;
    
    if capture_system.is_some() {
        return Err("Audio capture already running".to_string());
    }
    
    // Initialize speech recognizer
    let mut recognizer_guard = SPEECH_RECOGNIZER.lock().map_err(|e| e.to_string())?;
    if recognizer_guard.is_none() {
        let mut recognizer = SpeechRecognizer::new().map_err(|e| e.to_string())?;
        recognizer.initialize(None).map_err(|e| e.to_string())?;
        *recognizer_guard = Some(Arc::new(Mutex::new(recognizer)));
    }
    let recognizer = recognizer_guard.as_ref().unwrap().clone();
    drop(recognizer_guard);
    
    let system = Arc::new(AudioCaptureSystem::new().map_err(|e| e.to_string())?);
    let system_clone = Arc::clone(&system);
    
    // Start capture in background thread
    let window_clone = window.clone();
    let window_clone2 = window.clone();
    
    thread::spawn(move || {
        let mut audio_buffer = Vec::new();
        let buffer_duration_ms = 2000; // Reduced from 3000ms to 2000ms for faster response
        let target_sample_rate = 16000.0; // Target sample rate for Whisper
        let source_sample_rate = 48000.0; // Source sample rate from audio capture
        let samples_per_buffer = (target_sample_rate * buffer_duration_ms as f32 / 1000.0) as usize;
        
        if let Err(e) = system_clone.start_capture_with_device(device_name.clone(), move |audio_data| {
            // Process audio data and emit events
            let level = calculate_audio_level(&audio_data);
            let audio_level = AudioLevel {
                level,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            };
            
            // Debug log audio level
            if level > 0.01 {
                info!("Audio level: {:.4} (samples: {})", level, audio_data.len());
            }
            
            // Emit audio level to frontend
            if let Err(e) = window_clone.emit("audio-level", &audio_level) {
                error!("Failed to emit audio level: {}", e);
            }
            
            // Convert stereo to mono
            let mono_data = if audio_data.len() % 2 == 0 {
                audio_data.chunks_exact(2)
                    .map(|chunk| (chunk[0] + chunk[1]) / 2.0)
                    .collect::<Vec<f32>>()
            } else {
                audio_data.to_vec()
            };
            
            // Simple linear resampling from 48kHz to 16kHz (1/3 ratio)
            let resampled_data: Vec<f32> = mono_data.iter()
                .step_by(3) // Take every 3rd sample
                .copied()
                .collect();
            
            // Add to buffer
            audio_buffer.extend_from_slice(&resampled_data);
            
            // Process when buffer is full
            if audio_buffer.len() >= samples_per_buffer {
                let chunk = audio_buffer.drain(..samples_per_buffer).collect::<Vec<f32>>();
                
                // Enhanced voice activity detection with adjusted threshold
                let voice_detected = detect_voice_activity(&chunk, 0.0005);
                
                if level > 0.02 { // Lowered from 0.05 for more sensitive detection
                    info!("Audio detected - level: {:.4}, voice_activity: {}, samples: {}", level, voice_detected, chunk.len());
                    
                    if voice_detected {
                        let recognizer_clone = recognizer.clone();
                        let window_clone_inner = window_clone2.clone();
                        
                        // Process transcription in separate thread to avoid blocking
                        thread::spawn(move || {
                            info!("Starting transcription for audio chunk...");
                            if let Ok(recognizer_lock) = recognizer_clone.lock() {
                                match recognizer_lock.transcribe_audio(&chunk) {
                                    Ok(result) => {
                                        info!("Raw transcription result: '{}' (confidence: {:.2})", result.text, result.confidence);
                                        
                                        // Filter out non-speech results and low confidence
                                        if !result.text.trim().is_empty() && 
                                           result.confidence > 0.1 && // Lowered confidence threshold from 0.2 to 0.1
                                           !is_noise_transcription(&result.text) {
                                            info!("Accepted transcription: {} (confidence: {:.2})", result.text, result.confidence);
                                            if let Err(e) = window_clone_inner.emit("transcription-result", &result) {
                                                error!("Failed to emit transcription: {}", e);
                                            }
                                        } else {
                                            info!("Filtered out transcription: '{}' (confidence: {:.2}, is_noise: {}, is_empty: {})", 
                                                result.text, 
                                                result.confidence, 
                                                is_noise_transcription(&result.text),
                                                result.text.trim().is_empty()
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        error!("Transcription error: {}", e);
                                    }
                                }
                            }
                        });
                    } else {
                        info!("Audio detected but no voice activity - level: {:.4}", level);
                    }
                } else if level > 0.01 {
                    info!("Low audio level detected: {:.4}", level);
                }
            }
        }) {
            error!("Audio capture error: {}", e);
        }
    });
    
    *capture_system = Some(system);
    
    Ok("Audio capture and transcription started".to_string())
}

#[tauri::command]
async fn stop_audio_capture() -> Result<String, String> {
    info!("Stopping audio capture...");
    
    let mut capture_system = CAPTURE_SYSTEM.lock().map_err(|e| e.to_string())?;
    
    if let Some(system) = capture_system.take() {
        system.stop_capture().map_err(|e| e.to_string())?;
        Ok("Audio capture and transcription stopped".to_string())
    } else {
        Err("Audio capture not running".to_string())
    }
}

#[tauri::command]
async fn get_audio_devices() -> Result<Vec<String>, String> {
    info!("Getting audio devices...");
    AudioCaptureSystem::get_available_devices().map_err(|e| e.to_string())
}

#[tauri::command]
async fn check_permissions() -> Result<bool, String> {
    info!("Checking audio permissions...");
    AudioCaptureSystem::check_permissions().map_err(|e| e.to_string())
}

#[tauri::command]
async fn request_permissions() -> Result<bool, String> {
    info!("Requesting audio permissions...");
    AudioCaptureSystem::request_permissions().map_err(|e| e.to_string())
}

fn calculate_audio_level(audio_data: &[f32]) -> f64 {
    if audio_data.is_empty() {
        return 0.0;
    }
    
    // Calculate RMS (Root Mean Square) for audio level
    let rms: f64 = audio_data.iter()
        .map(|&sample| (sample as f64).powi(2))
        .sum::<f64>() / audio_data.len() as f64;
    
    let rms_value = rms.sqrt();
    
    // Apply amplification factor to make levels more visible
    // Audio samples are typically normalized between -1.0 and 1.0
    // But actual speech/music levels are often much lower
    let amplified = rms_value * 10.0; // Amplify by 10x
    
    // Clamp to reasonable range
    amplified.min(1.0)
}

fn detect_voice_activity(audio_data: &[f32], threshold: f64) -> bool {
    if audio_data.is_empty() {
        return false;
    }
    
    // Calculate energy
    let energy: f64 = audio_data.iter()
        .map(|&sample| (sample as f64).powi(2))
        .sum::<f64>() / audio_data.len() as f64;
    
    // Calculate zero crossing rate
    let zero_crossings = audio_data.windows(2)
        .filter(|window| (window[0] > 0.0) != (window[1] > 0.0))
        .count();
    
    let zcr = zero_crossings as f64 / audio_data.len() as f64;
    
    // Simple voice activity detection based on energy and zero crossing rate
    // Voice typically has moderate energy and moderate zero crossing rate
    // Music/noise often has higher energy or very high/low ZCR
    energy > threshold && zcr > 0.01 && zcr < 0.3
}

fn is_noise_transcription(text: &str) -> bool {
    let text_lower = text.to_lowercase();
    
    // Common noise patterns in Portuguese and English - more permissive now
    let noise_patterns = [
        "[", "]", "(", ")", "♪", "♫", "♬", "♭", "♯"
    ];
    
    // Check if text contains noise indicators
    for pattern in &noise_patterns {
        if text_lower.contains(pattern) {
            info!("Filtering out noise pattern: '{}' in '{}'", pattern, text);
            return true;
        }
    }
    
    // Check for very short transcriptions (likely noise)
    if text.trim().len() < 1 {  // Changed from 2 to 1
        info!("Filtering out very short transcription: '{}'", text);
        return true;
    }
    
    // Check for repetitive patterns (like "a a a a")
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() > 3 {  // Changed from 2 to 3
        let first_word = words[0];
        let repetitions = words.iter().filter(|&&word| word == first_word).count();
        if repetitions > words.len() * 3 / 4 {  // Changed from 1/2 to 3/4
            info!("Filtering out repetitive pattern: '{}'", text);
            return true;
        }
    }
    
    false
}

#[tauri::command]
async fn find_system_audio_device() -> Result<Option<String>, String> {
    info!("Finding system audio devices...");
    SystemAudioHelper::find_system_audio_device().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_device_info() -> Result<String, String> {
    info!("Getting detailed device information...");
    SystemAudioHelper::get_device_info().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_system_audio_setup() -> Result<String, String> {
    Ok(SystemAudioHelper::get_setup_instructions())
}

#[tauri::command]
async fn get_interview_response(transcription: String) -> Result<String, String> {
    info!("Getting interview response for: {}", transcription);
    
    // Embed the prompt content directly
    let context = include_str!("../../prompt.md");
    
    let gemini = GeminiService::new(GEMINI_API_KEY.to_string(), context.to_string());
    
    gemini.get_interview_response(&transcription)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            start_audio_capture,
            stop_audio_capture,
            get_audio_devices,
            check_permissions,
            request_permissions,
            find_system_audio_device,
            get_device_info,
            get_system_audio_setup,
            get_interview_response,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
