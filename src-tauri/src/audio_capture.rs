use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};
use log::{info, error, warn};

pub type AudioCallback = Box<dyn FnMut(&[f32]) + Send>;

pub struct AudioCaptureSystem {
    is_running: Arc<Mutex<bool>>,
    sample_rate: f64,
    channels: u32,
    buffer_size: u32,
}

impl AudioCaptureSystem {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        info!("Initializing Core Audio capture system...");
        
        Ok(Self {
            is_running: Arc::new(Mutex::new(false)),
            sample_rate: 48000.0,
            channels: 2,
            buffer_size: 1024,
        })
    }

    pub fn start_capture<F>(&self, callback: F) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        self.start_capture_with_device(None, callback)
    }

    pub fn start_capture_with_device<F>(&self, device_name: Option<String>, callback: F) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        let mut running = self.is_running.lock().unwrap();
        if *running {
            return Err("Capture already running".into());
        }
        *running = true;

        let is_running = Arc::clone(&self.is_running);
        let sample_rate = self.sample_rate;
        let channels = self.channels;
        let buffer_size = self.buffer_size;

        info!("Starting Core Audio capture with sample rate: {}, channels: {}, buffer size: {}", 
              sample_rate, channels, buffer_size);

        thread::spawn(move || {
            if let Err(e) = Self::capture_loop(is_running, sample_rate, channels, buffer_size, device_name, callback) {
                error!("Audio capture loop error: {}", e);
            }
        });

        Ok(())
    }

    pub fn stop_capture(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut running = self.is_running.lock().unwrap();
        *running = false;
        info!("Audio capture stopped");
        Ok(())
    }

    fn capture_loop<F>(
        is_running: Arc<Mutex<bool>>,
        sample_rate: f64,
        channels: u32,
        buffer_size: u32,
        device_name: Option<String>,
        callback: F,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        let host = cpal::default_host();
        
        // Find the specified device or use default
        let device = if let Some(name) = device_name {
            let devices = host.input_devices()?;
            let mut found_device = None;
            
            for device in devices {
                if let Ok(device_name_check) = device.name() {
                    // Check for exact match or partial match (for BlackHole variants)
                    if device_name_check == name || 
                       (name.contains("BlackHole") && device_name_check.contains("BlackHole")) ||
                       (name.contains("System Audio") && device_name_check.contains("BlackHole")) {
                        found_device = Some(device);
                        break;
                    }
                }
            }
            
            found_device.ok_or_else(|| format!("Device '{}' not found", name))?
        } else {
            host.default_input_device()
                .ok_or("No default input device available")?
        };
        
        info!("Using audio device: {}", device.name()?);
        
        let config = cpal::StreamConfig {
            channels: channels as cpal::ChannelCount,
            sample_rate: cpal::SampleRate(sample_rate as u32),
            buffer_size: cpal::BufferSize::Fixed(buffer_size),
        };

        let is_running_clone = Arc::clone(&is_running);
        let callback = Arc::new(Mutex::new(callback));
        let callback_clone = Arc::clone(&callback);
        
        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Check if we should continue running
                if let Ok(running) = is_running_clone.lock() {
                    if !*running {
                        return;
                    }
                }
                
                // Process the audio data
                if let Ok(mut cb) = callback_clone.lock() {
                    cb(data);
                }
            },
            |err| {
                error!("Audio stream error: {}", err);
            },
            None, // No timeout
        )?;

        stream.play()?;

        // Keep the stream alive while capture is running
        while *is_running.lock().unwrap() {
            thread::sleep(Duration::from_millis(100));
        }

        Ok(())
    }

    pub fn get_available_devices() -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let devices = host.input_devices()?;
        
        let mut device_names = Vec::new();
        for device in devices {
            match device.name() {
                Ok(name) => {
                    // Mark system audio devices specially
                    if name.to_lowercase().contains("blackhole") ||
                       name.to_lowercase().contains("aggregate") ||
                       name.to_lowercase().contains("multi") {
                        device_names.push(format!("{} (System Audio)", name));
                    } else {
                        device_names.push(name);
                    }
                },
                Err(e) => warn!("Failed to get device name: {}", e),
            }
        }
        
        Ok(device_names)
    }

    pub fn check_permissions() -> Result<bool, Box<dyn std::error::Error>> {
        // On macOS, we need to check if we have microphone permissions
        // For system audio capture, we'd need additional entitlements
        info!("Checking audio permissions...");
        
        // Try to access the default input device to check permissions
        let host = cpal::default_host();
        match host.default_input_device() {
            Some(_device) => {
                info!("Audio permissions appear to be granted");
                
                // Also check if BlackHole or system audio devices are available
                if let Ok(devices) = Self::get_available_devices() {
                    let system_audio_available = devices.iter().any(|d| 
                        d.to_lowercase().contains("blackhole") ||
                        d.to_lowercase().contains("aggregate") ||
                        d.to_lowercase().contains("system audio")
                    );
                    
                    if system_audio_available {
                        info!("System audio capture devices detected (BlackHole/Aggregate)");
                    } else {
                        info!("Only microphone devices available. Install BlackHole for system audio capture.");
                    }
                }
                
                Ok(true)
            }
            None => {
                warn!("No default input device available - permissions may be denied");
                Ok(false)
            }
        }
    }

    pub fn request_permissions() -> Result<bool, Box<dyn std::error::Error>> {
        info!("Requesting audio permissions...");
        
        // On macOS, permissions are typically requested automatically when
        // we try to access the microphone. For system audio capture,
        // we'd need to use AVCaptureSession or similar APIs.
        
        // For now, just check if we can access the device
        Self::check_permissions()
    }
}

// Core Audio Taps implementation for system audio capture
// This would be used for true system audio capture (like OBS)
pub struct CoreAudioTaps {
    // This would contain the Core Audio Taps implementation
    // for capturing system audio directly from macOS
}

impl CoreAudioTaps {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Implementation for macOS 14.4+ Audio Taps
        // This requires the com.apple.security.device.audio-input entitlement
        // and appropriate Info.plist entries
        
        info!("Core Audio Taps not yet implemented - using default input capture");
        Ok(Self {})
    }
    
    pub fn start_system_capture<F>(&self, _callback: F) -> Result<(), Box<dyn std::error::Error>>
    where
        F: Fn(&[f32]) + Send + Sync + 'static,
    {
        // This would implement the actual system audio capture
        // using Core Audio Taps API on macOS 14.4+
        
        Err("Core Audio Taps system capture not yet implemented".into())
    }
}