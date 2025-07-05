# Configuração de Áudio do Sistema - BlackHole

## 🎯 **Objetivo**
Capturar o áudio que está tocando no computador (como OBS faz) em vez de apenas o microfone.

## ✅ **Status do BlackHole**
BlackHole 2ch está parcialmente instalado no sistema.

## 📋 **Configuração Manual Necessária**

### **1. Verificar Instalação do BlackHole**
```bash
# Verificar se está instalado
system_profiler SPAudioDataType | grep -i black
```

### **2. Configurar Multi-Output Device (Dispositivo de Múltiplas Saídas)**

1. **Abrir Audio MIDI Setup**:
   - Pressione `Cmd + Espaço`
   - Digite "Audio MIDI Setup"
   - Pressione Enter

2. **Criar Multi-Output Device**:
   - Clique no `+` no canto inferior esquerdo
   - Selecione "Create Multi-Output Device"
   - Marque as caixas para:
     - ✅ **Built-in Output** (alto-falantes internos)
     - ✅ **BlackHole 2ch**

3. **Configurar como Master Device**:
   - Clique com botão direito no Multi-Output Device
   - Selecione "Use This Device For Sound Output"

### **3. Configurar Aggregate Device (Dispositivo Agregado)**

1. **Criar Aggregate Device**:
   - No Audio MIDI Setup, clique `+`
   - Selecione "Create Aggregate Device"
   - Marque as caixas para:
     - ✅ **BlackHole 2ch** 
     - ✅ **Built-in Microphone** (se quiser microfone também)

2. **Renomear o Device**:
   - Nomeie como "System Audio + Mic"

### **4. Configurar Sistema**

1. **Saída de Áudio**:
   - System Preferences → Sound → Output
   - Selecione "Multi-Output Device"

2. **Entrada de Áudio**:
   - System Preferences → Sound → Input  
   - Selecione "Aggregate Device" ou "BlackHole 2ch"

## 🔧 **Código Atualizado para Suporte ao BlackHole**

### **Detecção Automática do BlackHole**
```rust
pub fn find_blackhole_device() -> Result<Option<String>, Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let devices = host.input_devices()?;
    
    for device in devices {
        let name = device.name()?;
        if name.to_lowercase().contains("blackhole") || 
           name.to_lowercase().contains("aggregate") {
            info!("Found system audio device: {}", name);
            return Ok(Some(name));
        }
    }
    
    info!("BlackHole not found, using default input device");
    Ok(None)
}
```

### **Comando para Listar Dispositivos Disponíveis**
```rust
#[tauri::command]
async fn get_system_audio_devices() -> Result<Vec<String>, String> {
    AudioCaptureSystem::get_system_audio_devices().map_err(|e| e.to_string())
}
```

## 🎵 **Como Usar**

### **Para Capturar Áudio do Sistema (como música, YouTube, etc.)**:
1. Configure o Multi-Output Device (passos acima)
2. No DevCaption, selecione "BlackHole 2ch" ou "Aggregate Device"
3. Toque música/vídeo no computador
4. Clique "Start Recording" no DevCaption
5. O áudio do sistema será transcrito em tempo real

### **Para Capturar Áudio de Aplicações Específicas**:
1. Abra a aplicação (ex: Spotify, YouTube)
2. Configure a saída da aplicação para "BlackHole 2ch"
3. Use "Built-in Output" para ouvir normalmente
4. DevCaption capturará apenas o áudio da aplicação específica

## ⚠️ **Troubleshooting**

### **Não ouço mais áudio**:
- System Preferences → Sound → Output
- Mude de volta para "Built-in Output" ou "External Headphones"

### **BlackHole não aparece nos dispositivos**:
```bash
# Reinstalar BlackHole (requer sudo)
brew reinstall blackhole-2ch
# OU baixar direto de: https://existential.audio/blackhole/
```

### **Áudio cortado ou com ruído**:
- No Audio MIDI Setup, configure sample rate para 48kHz
- Verifique se o buffer size está adequado

### **DevCaption não detecta o BlackHole**:
- Reinicie o DevCaption
- Verifique se o device está selecionado corretamente
- Teste com `get_audio_devices` command

## 🎯 **Resultado Esperado**

Com essa configuração, você conseguirá:
- ✅ **Capturar áudio do sistema** (como OBS faz)
- ✅ **Transcrever música, vídeos, podcasts** em tempo real
- ✅ **Capturar áudio de aplicações específicas**
- ✅ **Manter compatibilidade** com microfone normal
- ✅ **Funcionar offline** sem dependências externas

## 📱 **Casos de Uso**

### **Transcrição de Conteúdo Online**:
- YouTube videos → Legendas automáticas
- Podcasts → Transcrições de texto
- Calls do Zoom/Teams → Atas de reunião
- Streaming de música → Letras em tempo real

### **Acessibilidade**:
- Pessoas com deficiência auditiva
- Legendas ao vivo para apresentações
- Transcrição de áudio em idiomas estrangeiros

Essa configuração transforma o DevCaption em uma ferramenta poderosa para captura de **qualquer áudio** do sistema! 🚀