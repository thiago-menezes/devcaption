use cpal::traits::{HostTrait, DeviceTrait};
use log::{info, warn};

pub struct SystemAudioHelper;

impl SystemAudioHelper {
    pub fn find_system_audio_device() -> Result<Option<String>, Box<dyn std::error::Error>> {
        info!("Searching for system audio devices (BlackHole, Aggregate, etc.)...");
        
        let host = cpal::default_host();
        let devices = host.input_devices()?;
        
        // Priority order: BlackHole > Aggregate > Multi-Output
        let mut found_devices = Vec::new();
        
        for device in devices {
            if let Ok(name) = device.name() {
                let name_lower = name.to_lowercase();
                if name_lower.contains("blackhole") {
                    found_devices.push((1, name.clone())); // Highest priority
                } else if name_lower.contains("aggregate") {
                    found_devices.push((2, name.clone()));
                } else if name_lower.contains("multi") {
                    found_devices.push((3, name.clone()));
                }
            }
        }
        
        // Sort by priority and return the best option
        found_devices.sort_by_key(|&(priority, _)| priority);
        
        if let Some((_, device_name)) = found_devices.first() {
            info!("Found system audio device: {}", device_name);
            Ok(Some(device_name.clone()))
        } else {
            info!("No system audio devices found. Install BlackHole for system audio capture.");
            Ok(None)
        }
    }

    pub fn get_device_info() -> Result<String, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let devices = host.input_devices()?;
        
        let mut info = String::new();
        info.push_str("Available Audio Input Devices:\n");
        
        for (i, device) in devices.enumerate() {
            if let Ok(name) = device.name() {
                let device_type = if name.to_lowercase().contains("blackhole") ||
                                    name.to_lowercase().contains("aggregate") ||
                                    name.to_lowercase().contains("multi") {
                    " [SYSTEM AUDIO]"
                } else {
                    " [MICROPHONE]"
                };
                
                info.push_str(&format!("{}. {}{}", i + 1, name, device_type));
                
                // Try to get supported formats
                if let Ok(supported_configs) = device.supported_input_configs() {
                    let configs: Vec<_> = supported_configs.collect();
                    if !configs.is_empty() {
                        info.push_str(&format!(" (Channels: {}, Sample Rate: {} Hz)", 
                            configs[0].channels(),
                            configs[0].min_sample_rate().0
                        ));
                    }
                }
                info.push('\n');
            }
        }
        
        Ok(info)
    }

    pub fn is_system_audio_device(device_name: &str) -> bool {
        let name_lower = device_name.to_lowercase();
        name_lower.contains("blackhole") ||
        name_lower.contains("aggregate") ||
        name_lower.contains("multi") ||
        name_lower.contains("system audio")
    }

    pub fn get_setup_instructions() -> String {
        String::from("🎵 CONFIGURAÇÃO PARA ÁUDIO DO SISTEMA (como OBS)

1. INSTALAR BLACKHOLE:
   - Download: https://existential.audio/blackhole/
   - Ou: brew install blackhole-2ch

2. CONFIGURAR MULTI-OUTPUT DEVICE:
   - Abrir \"Audio MIDI Setup\"
   - Criar \"Multi-Output Device\"
   - Marcar: Built-in Output + BlackHole 2ch
   - Definir como saída padrão do sistema

3. CONFIGURAR AGGREGATE DEVICE:
   - Criar \"Aggregate Device\"
   - Marcar: BlackHole 2ch + Built-in Microphone
   - Usar este device no DevCaption

4. USAR NO DEVCAPTION:
   - Selecionar device com \"[SYSTEM AUDIO]\"
   - Tocar música/vídeo no computador
   - Clicar \"Start Recording\"
   - O áudio do sistema será transcrito!

💡 CASOS DE USO:
   ✅ Transcrever YouTube, Spotify, Podcasts
   ✅ Legendas ao vivo para vídeos
   ✅ Atas de reuniões do Zoom/Teams
   ✅ Transcrições offline e privadas")
    }
}