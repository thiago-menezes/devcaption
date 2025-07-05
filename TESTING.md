# Testing DevCaption

## Quick Test Guide

### 1. **Start the Application**
```bash
npm run tauri dev
```

### 2. **Check Model Loading**
- Watch the terminal output for model loading messages
- You should see: `"Whisper model loaded successfully from: [path]"`

### 3. **Test Audio Capture**
1. **Grant Permissions**: Click "Allow" when prompted for microphone access
2. **Check Status**: Verify "✓ Audio permissions granted" appears
3. **Select Device**: Choose your microphone from the dropdown
4. **Start Recording**: Click the "🎤 Start Recording" button

### 4. **Test Transcription**
1. **Speak Clearly**: Say something like "Hello, this is a test of the transcription system"
2. **Watch Audio Levels**: The blue audio meter should show activity
3. **Wait for Transcription**: Text should appear within 3-5 seconds
4. **Check Confidence**: Look for confidence scores next to the text

### 5. **Test Export**
1. **Add Some Text**: Speak a few sentences
2. **Click Export**: Use the "💾 Export" button
3. **Download File**: A text file should download automatically

## Troubleshooting

### **Model Not Found Error**
If you see: `"Whisper model not found"`
- Check that `ggml-base.en.bin` exists in both:
  - `/models/ggml-base.en.bin`
  - `/src-tauri/ggml-base.en.bin`

### **No Audio Detection**
- Check microphone permissions in System Preferences
- Ensure microphone is not muted
- Try speaking louder or closer to the microphone
- Check that the correct device is selected

### **No Transcription Appearing**
- Wait 3-5 seconds after speaking
- Check the terminal for transcription log messages
- Ensure audio levels are showing (blue meter)
- Try speaking more clearly or in English

### **Performance Issues**
- Close other audio applications
- Reduce background processes
- Check CPU usage - Whisper processing is CPU-intensive

## Expected Behavior

### **Audio Capture**
- ✅ Audio level meter shows real-time activity
- ✅ Recording button changes to red when active
- ✅ Status shows "🔴 Recording" at the bottom

### **Transcription**
- ✅ Text appears 3-5 seconds after speaking
- ✅ Confidence scores show (typically 60-95%)
- ✅ New text appends to previous transcriptions
- ✅ Real-time updates during speech

### **UI Behavior**
- ✅ Professional dark theme
- ✅ Smooth animations and transitions
- ✅ Responsive layout on different window sizes
- ✅ Clear error messages if issues occur

## Sample Test Phrases

Try these phrases to test different aspects:

### **Basic Test**
"Hello, this is a test of the speech recognition system."

### **Technical Terms**
"I am testing the audio transcription with technical words like algorithm, database, and authentication."

### **Numbers and Punctuation**
"Today is July 5th, 2025, and the time is approximately 3:30 PM."

### **Longer Sentences**
"This application demonstrates real-time speech-to-text conversion using OpenAI's Whisper model running locally on the device for privacy and offline functionality."

## Success Criteria

✅ **Application launches without errors**  
✅ **Model loads successfully (check terminal)**  
✅ **Microphone permissions granted**  
✅ **Audio levels show when speaking**  
✅ **Transcription appears within 5 seconds**  
✅ **Text export works correctly**  
✅ **No memory leaks or crashes during use**

## Performance Benchmarks

### **Expected Performance**
- **Model Loading**: 2-5 seconds
- **Transcription Latency**: 1-3 seconds
- **Memory Usage**: <100MB
- **CPU Usage**: 10-30% during transcription
- **Accuracy**: 85-95% for clear English speech

### **Optimization Notes**
- 3-second audio chunks for optimal accuracy/latency balance
- Voice activity detection reduces unnecessary processing
- Multi-threaded processing keeps UI responsive
- Greedy decoding for faster (vs beam search) transcription