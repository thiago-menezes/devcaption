import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

interface TranscriptionResult {
  text: string;
  confidence: number;
  timestamp: number;
  is_final: boolean;
}

interface AudioLevel {
  level: number;
  timestamp: number;
}

interface ResponseHistory {
  question: string;
  response: string;
  timestamp: number;
}

// interface AudioDevice {
//   name: string;
//   id: string;
// }

function App() {
  const [isRecording, setIsRecording] = useState(false);
  const [transcriptionText, setTranscriptionText] = useState("");
  const [currentTranscription, setCurrentTranscription] = useState("");
  const [lastProcessedText, setLastProcessedText] = useState("");
  const [responseHistory, setResponseHistory] = useState<ResponseHistory[]>([]);
  const [isLoadingResponse, setIsLoadingResponse] = useState(false);
  const [audioLevel, setAudioLevel] = useState(0);
  const [audioDevices, setAudioDevices] = useState<string[]>([]);
  const [selectedDevice, setSelectedDevice] = useState<string>("");
  const [hasPermissions, setHasPermissions] = useState(false);
  const [isCheckingPermissions, setIsCheckingPermissions] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confidence, setConfidence] = useState(0);
  const [geminiResponse, setGeminiResponse] = useState<string>("");
  const [debugLog, setDebugLog] = useState<string[]>([]);
  const [currentResponse, setCurrentResponse] = useState<string>("");
  
  const transcriptionRef = useRef<HTMLDivElement>(null);
  const audioLevelRef = useRef<HTMLDivElement>(null);

  const addDebugLog = (message: string) => {
    console.log(message);
    setDebugLog(prev => [...prev, `${new Date().toISOString()}: ${message}`]);
  };

  useEffect(() => {
    // Check permissions on startup
    checkPermissions();
    
    // Get available audio devices
    getAudioDevices();
    
    // Listen for audio level updates
    addDebugLog("Setting up event listeners");

    const unlisten = listen<AudioLevel>('audio-level', (event) => {
      const { level } = event.payload;
      setAudioLevel(level);
      
      // Update audio level visualization
      if (audioLevelRef.current) {
        const percentage = Math.min(level * 100, 100);
        audioLevelRef.current.style.width = `${percentage}%`;
      }
    });

    // Listen for transcription results
    const unlistenTranscription = listen<TranscriptionResult>('transcription-result', (event) => {
      const { text, confidence, is_final } = event.payload;
      
      if (is_final) {
        setTranscriptionText(prev => prev + " " + text);
        setCurrentTranscription("");
      } else {
        setCurrentTranscription(text);
      }
      
      setConfidence(confidence);
      
      // Auto-scroll to bottom
      if (transcriptionRef.current) {
        transcriptionRef.current.scrollTop = transcriptionRef.current.scrollHeight;
      }
    });

    return () => {
      unlisten.then(f => f());
      unlistenTranscription.then(f => f());
      addDebugLog("Event listeners cleaned up");
    };
  }, []);

  const checkPermissions = async () => {
    setIsCheckingPermissions(true);
    try {
      const result = await invoke<boolean>("check_permissions");
      setHasPermissions(result);
    } catch (err) {
      setError(`Permission check failed: ${err}`);
    } finally {
      setIsCheckingPermissions(false);
    }
  };

  const requestPermissions = async () => {
    try {
      const result = await invoke<boolean>("request_permissions");
      setHasPermissions(result);
      if (!result) {
        setError("Audio permissions are required for this application to work");
      }
    } catch (err) {
      setError(`Permission request failed: ${err}`);
    }
  };

  const getAudioDevices = async () => {
    try {
      const devices = await invoke<string[]>("get_audio_devices");
      setAudioDevices(devices);
      if (devices.length > 0 && !selectedDevice) {
        setSelectedDevice(devices[0]);
      }
    } catch (err) {
      setError(`Failed to get audio devices: ${err}`);
    }
  };

  const startRecording = async () => {
    if (!hasPermissions) {
      await requestPermissions();
      return;
    }

    try {
      setError(null);
      await invoke("start_audio_capture", { deviceName: selectedDevice || null });
      setIsRecording(true);
    } catch (err) {
      setError(`Failed to start recording: ${err}`);
    }
  };

  const stopRecording = async () => {
    try {
      await invoke("stop_audio_capture");
      setIsRecording(false);
      setAudioLevel(0);
      if (audioLevelRef.current) {
        audioLevelRef.current.style.width = "0%";
      }
    } catch (err) {
      setError(`Failed to stop recording: ${err}`);
    }
  };

  const getInterviewResponse = async () => {
    try {
      // Get the new text since last processing
      const newText = transcriptionText.slice(lastProcessedText.length).trim();
      
      if (!newText) {
        setError("No new text to process");
        return;
      }

      setIsLoadingResponse(true);
      const response = await invoke<string>("get_interview_response", { transcription: newText });
      
      // Add to history
      setResponseHistory(prev => [...prev, {
        question: newText,
        response: response,
        timestamp: Date.now()
      }]);

      // Update last processed text
      setLastProcessedText(transcriptionText);
    } catch (err) {
      setError(`Failed to get interview response: ${err}`);
    } finally {
      setIsLoadingResponse(false);
    }
  };

  return (
    <main className="app">
      <header className="app-header">
        <h1>DevCaption</h1>
        <p>Real-time Audio Transcription</p>
      </header>

      <div className="controls-panel">
        <div className="permission-status">
          {isCheckingPermissions ? (
            <span className="status checking">Checking permissions...</span>
          ) : hasPermissions ? (
            <span className="status granted">✓ Audio permissions granted</span>
          ) : (
            <span className="status denied">⚠ Audio permissions required</span>
          )}
        </div>

        <div className="audio-controls">
          <button 
            className={`record-button ${isRecording ? 'recording' : ''}`}
            onClick={isRecording ? stopRecording : startRecording}
            disabled={isCheckingPermissions}
          >
            {isRecording ? '⏹ Stop Recording' : '🎤 Start Recording'}
          </button>
          
          <button 
            className="process-button"
            onClick={getInterviewResponse}
            disabled={!transcriptionText || isLoadingResponse || transcriptionText === lastProcessedText}
          >
            {isLoadingResponse ? '⏳ Processing...' : '💭 Get Response'}
          </button>
          
          <button 
            className="clear-button"
            onClick={() => {
              setTranscriptionText("");
              setLastProcessedText("");
              setCurrentTranscription("");
            }}
            disabled={!transcriptionText}
          >
            🗑 Clear
          </button>
          
          <button 
            className="export-button"
            onClick={() => {
              if (!transcriptionText.trim()) {
                setError("No transcription to export");
                return;
              }
              
              const blob = new Blob([transcriptionText], { type: 'text/plain' });
              const url = URL.createObjectURL(blob);
              const a = document.createElement('a');
              a.href = url;
              a.download = `transcription-${Date.now()}.txt`;
              document.body.appendChild(a);
              a.click();
              document.body.removeChild(a);
              URL.revokeObjectURL(url);
            }}
            disabled={!transcriptionText.trim()}
          >
            💾 Export
          </button>
        </div>

        {/* Audio Level Meter */}
        <div className="audio-level-container">
          <label>Audio Level:</label>
          <div className="audio-level-meter">
            <div 
              ref={audioLevelRef}
              className="audio-level-fill"
            ></div>
          </div>
          <span className="audio-level-text">{(audioLevel * 100).toFixed(1)}%</span>
        </div>

        {/* Device Selection */}
        <div className="device-selection">
          <label htmlFor="device-select">Audio Device:</label>
          <select 
            id="device-select"
            value={selectedDevice}
            onChange={(e) => setSelectedDevice(e.target.value)}
            disabled={isRecording}
          >
            {audioDevices.map((device, index) => (
              <option key={index} value={device}>
                {device}
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* Transcription Display */}
      <div className="panels-container">
        <div className="transcription-panel">
          <div className="transcription-header">
            <h2>Transcription</h2>
            {confidence > 0 && (
              <span className="confidence-indicator">
                Confidence: {(confidence * 100).toFixed(1)}%
              </span>
            )}
          </div>
          
          <div 
            ref={transcriptionRef}
            className="transcription-display"
          >
            {transcriptionText && (
              <div className="final-text">
                {transcriptionText}
              </div>
            )}
            {currentTranscription && (
              <div className="current-text">
                {currentTranscription}
              </div>
            )}
            {!transcriptionText && !currentTranscription && (
              <div className="placeholder-text">
                {isRecording 
                  ? "Listening... Speak into your microphone" 
                  : "Click 'Start Recording' to begin transcription"
                }
              </div>
            )}
          </div>
        </div>

        <div className="response-panel">
          <div className="response-header">
            <h2>Interview Responses</h2>
            {isLoadingResponse && (
              <span className="loading-indicator">
                Generating response...
              </span>
            )}
          </div>
          
          <div className="response-display">
            {responseHistory.length > 0 ? (
              <div className="response-history">
                {responseHistory.map((item, index) => (
                  <div key={item.timestamp} className="response-item">
                    <div className="response-question">
                      Q: {item.question}
                    </div>
                    <div className="response-text">
                      {item.response}
                    </div>
                    {index < responseHistory.length - 1 && (
                      <div className="response-divider" />
                    )}
                  </div>
                ))}
              </div>
            ) : (
              <div className="placeholder-text">
                Your interview responses will appear here
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Error Display */}
      {error && (
        <div className="error-panel">
          <span className="error-text">⚠ {error}</span>
          <button 
            className="error-dismiss"
            onClick={() => setError(null)}
          >
            ×
          </button>
        </div>
      )}

      {/* Status Bar */}
      <div className="status-bar">
        <span className="status-text">
          {isRecording ? '🔴 Recording' : '⚪ Ready'}
        </span>
        <span className="device-info">
          {selectedDevice && `Device: ${selectedDevice}`}
        </span>
      </div>
    </main>
  );
}

export default App;