use std::sync::{Arc, Mutex};
use std::thread;
use serde::{Deserialize, Serialize};
use log::{info, error};
use tauri::Emitter;
use serde_json;
use std::time::{SystemTime, UNIX_EPOCH, Duration, Instant};
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::mpsc;

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

// Add this near the top with other static variables
static LAST_TRANSCRIPTION_TIME: AtomicU64 = AtomicU64::new(0);
static TRANSCRIPTION_BUFFER: Mutex<String> = Mutex::new(String::new());
static CURRENT_SESSION_TEXT: Mutex<String> = Mutex::new(String::new());
static IS_RECORDING: AtomicBool = AtomicBool::new(false);
static LAST_VOICE_TIME: Mutex<Option<Instant>> = Mutex::new(None);
static RECORDING_START_TIME: Mutex<Option<Instant>> = Mutex::new(None);
static LAST_PARTIAL_PROCESSING: Mutex<Option<Instant>> = Mutex::new(None);
static IS_PROCESSING: AtomicBool = AtomicBool::new(false);

// Constants
const GEMINI_API_KEY: &str = "AIzaSyBzcVnMVBRXHGWbAhAaSQdoubc6YuLkcv8";
const SILENCE_THRESHOLD: f64 = 0.05; // 5% threshold (more sensitive to catch quiet speech)
const SILENCE_DELAY: Duration = Duration::from_millis(800); // 0.8s delay
const STREAMING_CHUNK_SIZE: usize = 80000; // ~5 seconds at 16kHz for streaming
const MIN_CHUNK_SIZE: usize = 16000; // ~1 second minimum before processing

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
        let buffer_duration_ms = 3000; // 3 seconds buffer
        let target_sample_rate = 16000.0;
        let source_sample_rate = 48000.0;
        let samples_per_buffer = (target_sample_rate * buffer_duration_ms as f32 / 1000.0) as usize;
        
        info!("Audio capture thread started. Buffer size: {} samples", samples_per_buffer);
        
        if let Err(e) = system_clone.start_capture_with_device(device_name.clone(), move |audio_data| {
            // Process audio data and emit events
            let level = calculate_audio_level(&audio_data);
            
            info!("Audio level: {:.6}", level);
            
            let audio_level = AudioLevel {
                level,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            };
            
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
            
            // Simple resampling
            let resampled_data: Vec<f32> = mono_data.iter()
                .step_by(3)
                .copied()
                .collect();
            
            // Check if there's voice activity
            let has_voice = level > SILENCE_THRESHOLD;
            let now = Instant::now();
            
            if has_voice {
                // Voice detected, start/continue recording
                if let Ok(mut last_voice_time) = LAST_VOICE_TIME.lock() {
                    *last_voice_time = Some(now);
                }
                
                if !IS_RECORDING.load(Ordering::Relaxed) {
                    info!("Voice detected, starting recording");
                    IS_RECORDING.store(true, Ordering::Relaxed);
                    audio_buffer.clear(); // Clear any old data
                    
                    // Reset session text for new recording
                    if let Ok(mut session_text) = CURRENT_SESSION_TEXT.lock() {
                        session_text.clear();
                    }
                    
                    // Set recording start time
                    if let Ok(mut recording_start_time) = RECORDING_START_TIME.lock() {
                        *recording_start_time = Some(now);
                    }
                    if let Ok(mut last_partial_processing) = LAST_PARTIAL_PROCESSING.lock() {
                        *last_partial_processing = Some(now);
                    }
                }
                
                // Add current data to buffer
                audio_buffer.extend_from_slice(&resampled_data);
                
                // Streaming processing: process chunks as we go for long speech
                if audio_buffer.len() >= STREAMING_CHUNK_SIZE && !IS_PROCESSING.load(Ordering::Relaxed) {
                    info!("Streaming mode: processing chunk with {} samples", STREAMING_CHUNK_SIZE);
                    
                    IS_PROCESSING.store(true, Ordering::Relaxed);
                    
                    // Take a chunk for processing, keep overlap for continuity
                    let overlap_size = 8000; // 0.5 second overlap
                    let chunk_to_process = audio_buffer[..STREAMING_CHUNK_SIZE].to_vec();
                    
                    // Remove processed part but keep overlap
                    audio_buffer.drain(..(STREAMING_CHUNK_SIZE - overlap_size));
                    
                    let recognizer_clone = recognizer.clone();
                    let window_clone_inner = window_clone2.clone();
                    
                    thread::spawn(move || {
                        process_audio_chunk(recognizer_clone, window_clone_inner, chunk_to_process, false);
                        IS_PROCESSING.store(false, Ordering::Relaxed);
                    });
                }
                
            } else {
                // No voice, check if we should stop recording
                if IS_RECORDING.load(Ordering::Relaxed) {
                    if let Ok(last_voice_time) = LAST_VOICE_TIME.lock() {
                        if let Some(last_time) = *last_voice_time {
                            let silence_duration = now.duration_since(last_time);
                            
                            if silence_duration >= SILENCE_DELAY {
                                info!("Silence detected for {:.2}s, stopping recording and processing", silence_duration.as_secs_f64());
                                IS_RECORDING.store(false, Ordering::Relaxed);
                                
                                // Process the accumulated audio - always process final chunk
                                if !audio_buffer.is_empty() && audio_buffer.len() >= MIN_CHUNK_SIZE {
                                    // Wait for current processing to finish, but don't block forever
                                    let mut wait_count = 0;
                                    while IS_PROCESSING.load(Ordering::Relaxed) && wait_count < 20 {
                                        thread::sleep(Duration::from_millis(100));
                                        wait_count += 1;
                                    }
                                    
                                    if !IS_PROCESSING.load(Ordering::Relaxed) {
                                        IS_PROCESSING.store(true, Ordering::Relaxed);
                                        
                                        // Move data instead of cloning
                                        let chunk_to_process = std::mem::replace(&mut audio_buffer, Vec::new());
                                        
                                        info!("Processing final accumulated audio with {} samples", chunk_to_process.len());
                                        
                                        let recognizer_clone = recognizer.clone();
                                        let window_clone_inner = window_clone2.clone();
                                        
                                        thread::spawn(move || {
                                            process_audio_chunk(recognizer_clone, window_clone_inner, chunk_to_process, true);
                                            IS_PROCESSING.store(false, Ordering::Relaxed);
                                        });
                                    } else {
                                        info!("Skipping final processing - still processing previous chunk");
                                    }
                                } else if !audio_buffer.is_empty() {
                                    info!("Skipping final processing - chunk too small: {} samples", audio_buffer.len());
                                    audio_buffer.clear(); // Clear small chunks
                                }
                            }
                        }
                    }
                } else {
                    // Not recording and no voice, just continue
                    // We could add the current audio to buffer for smoothness, but we'll skip it
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
        
        // Reset recording state
        IS_RECORDING.store(false, Ordering::Relaxed);
        IS_PROCESSING.store(false, Ordering::Relaxed);
        if let Ok(mut last_voice_time) = LAST_VOICE_TIME.lock() {
            *last_voice_time = None;
        }
        if let Ok(mut recording_start_time) = RECORDING_START_TIME.lock() {
            *recording_start_time = None;
        }
        if let Ok(mut last_partial_processing) = LAST_PARTIAL_PROCESSING.lock() {
            *last_partial_processing = None;
        }
        if let Ok(mut session_text) = CURRENT_SESSION_TEXT.lock() {
            session_text.clear();
        }
        
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

fn process_audio_chunk(recognizer: Arc<Mutex<SpeechRecognizer>>, window: tauri::Window, chunk_to_process: Vec<f32>, is_final: bool) {
    info!("Starting audio processing with {} samples", chunk_to_process.len());
    
    // Use channel for timeout
    let (tx, rx) = mpsc::channel();
    let recognizer_clone = recognizer.clone();
    
    // Spawn processing in separate thread
    thread::spawn(move || {
        let result = if let Ok(recognizer_lock) = recognizer_clone.try_lock() {
            match recognizer_lock.transcribe_audio(&chunk_to_process) {
                Ok(result) => Some(result),
                Err(e) => {
                    error!("Transcription error: {}", e);
                    None
                }
            }
        } else {
            error!("Failed to acquire recognizer lock - skipping this chunk");
            None
        };
        
        let _ = tx.send(result); // Send result or None
    });
    
    // Wait for result with timeout (increased for better reliability)
    match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(Some(result)) => {
            info!("Transcription result: '{}' (confidence: {:.2})", 
                result.text, result.confidence);
            
            let transcribed_text = result.text.trim().to_string();
            
            // Filter out unwanted results
            let should_skip = transcribed_text.is_empty() 
                || transcribed_text.contains("[BLANK_AUDIO]")
                || transcribed_text.trim() == "you"
                || transcribed_text.trim().matches("you").count() > 2;
            
            if !should_skip {
                // Handle text accumulation based on type
                if let Ok(mut session_text) = CURRENT_SESSION_TEXT.lock() {
                    if is_final {
                        // For final results, add to accumulated text permanently
                        if !session_text.is_empty() && !session_text.ends_with(' ') {
                            session_text.push(' ');
                        }
                        session_text.push_str(&transcribed_text);
                        
                        let final_result = TranscriptionResult {
                            text: session_text.clone(),
                            confidence: result.confidence,
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64,
                            is_final: true,
                        };
                        
                        info!("Sending FINAL accumulated transcription: {}", final_result.text);
                        if let Err(e) = window.emit("transcription-result", &final_result) {
                            error!("Failed to emit final transcription: {}", e);
                        }
                        
                        LAST_TRANSCRIPTION_TIME.store(final_result.timestamp, Ordering::Relaxed);
                    } else {
                        // For streaming partial results, add to accumulated text for display
                        // but don't make it permanent until final
                        if !session_text.is_empty() && !session_text.ends_with(' ') {
                            session_text.push(' ');
                        }
                        session_text.push_str(&transcribed_text);
                        
                        let partial_result = TranscriptionResult {
                            text: session_text.clone(),
                            confidence: result.confidence,
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64,
                            is_final: false,
                        };
                        
                        info!("Sending PARTIAL accumulated transcription: {}", partial_result.text);
                        if let Err(e) = window.emit("transcription-result", &partial_result) {
                            error!("Failed to emit partial transcription: {}", e);
                        }
                    }
                }
            } else {
                info!("Skipping unwanted result: {}", transcribed_text);
            }
        }
        Ok(None) => {
            info!("Transcription returned no result");
        }
        Err(_) => {
            error!("Transcription timeout after 15 seconds - skipping this chunk");
        }
    }
    
    info!("Audio processing completed");
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
    
    // Calculate energy with frequency weighting
    let weighted_energy: f64 = audio_data.iter()
        .enumerate()
        .map(|(i, &sample)| {
            let freq_weight = (i as f64 / audio_data.len() as f64).min(1.0);
            (sample as f64 * freq_weight).powi(2)
        })
        .sum::<f64>() / audio_data.len() as f64;
    
    // Calculate zero crossing rate with improved accuracy
    let zero_crossings = audio_data.windows(2)
        .filter(|window| {
            let sign_change = (window[0] > 0.0) != (window[1] > 0.0);
            let magnitude = (window[0] - window[1]).abs();
            sign_change && magnitude > 0.01 // Filter out tiny fluctuations
        })
        .count();
    
    let zcr = zero_crossings as f64 / audio_data.len() as f64;
    
    // Calculate spectral centroid (rough approximation)
    let spectral_sum: f64 = audio_data.iter()
        .enumerate()
        .map(|(i, &sample)| i as f64 * (sample as f64).abs())
        .sum::<f64>();
    
    let magnitude_sum: f64 = audio_data.iter()
        .map(|&sample| (sample as f64).abs())
        .sum::<f64>();
    
    let spectral_centroid = if magnitude_sum > 0.0 {
        spectral_sum / magnitude_sum / audio_data.len() as f64
    } else {
        0.0
    };
    
    // Improved voice activity detection using multiple features
    weighted_energy > threshold && // Energy check
    zcr > 0.01 && zcr < 0.35 && // More permissive ZCR range
    spectral_centroid > 0.1 && spectral_centroid < 0.3 // Typical range for speech
}

fn is_noise_transcription(text: &str) -> bool {
    let text_lower = text.to_lowercase();
    
    // Common noise patterns in Portuguese and English
    let noise_patterns = [
        "[", "]", "(", ")", "♪", "♫", "♬", "♭", "♯",
        "mmm", "uhh", "umm", "err", "ahh",
        "...", "///", "---"
    ];
    
    // Check if text contains noise indicators
    for pattern in &noise_patterns {
        if text_lower.contains(pattern) {
            info!("Filtering out noise pattern: '{}' in '{}'", pattern, text);
            return true;
        }
    }
    
    // Check for very short transcriptions (likely noise)
    if text.trim().len() < 2 {
        info!("Filtering out very short transcription: '{}'", text);
        return true;
    }
    
    // Check for repetitive patterns (like "a a a a")
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() > 3 {
        let first_word = words[0];
        let repetitions = words.iter().filter(|&&word| word == first_word).count();
        if repetitions > words.len() * 4 / 5 { // More permissive repetition check
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
async fn get_interview_response(transcription: String, is_first_question: bool) -> Result<String, String> {
    info!("Getting interview response for: {}", transcription);
    
    // Embed the prompt content directly
    let context = include_str!("../../prompt.md");
    
    let gemini = GeminiService::new(GEMINI_API_KEY.to_string(), context.to_string());
    
    gemini.get_interview_response(&transcription, is_first_question)
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
