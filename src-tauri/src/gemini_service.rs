use serde::{Deserialize, Serialize};
use log::{info, error};

#[derive(Debug, Serialize, Deserialize)]
pub struct GeminiRequest {
    contents: Vec<Content>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Content {
    parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Part {
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GeminiResponse {
    Success {
        candidates: Vec<Candidate>,
    },
    Error {
        error: GeminiError,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeminiError {
    code: i32,
    message: String,
    status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Candidate {
    content: Content,
}

pub struct GeminiService {
    api_key: String,
    base_url: String,
    context: String,
}

impl GeminiService {
    pub fn new(api_key: String, context: String) -> Self {
        Self {
            api_key,
            base_url: String::from("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent"),
            context,
        }
    }

    pub async fn get_interview_response(&self, transcription: &str, is_first_question: bool) -> Result<String, Box<dyn std::error::Error>> {
        info!("Getting interview response for transcription: {}", transcription);

        let client = reqwest::Client::new();
        
        // Base transcription note to include in all prompts
        let transcription_note = "Note: The question comes from real-time audio transcription, so there might be some noise or repetition in the text. Try to understand the core question even if there are small transcription artifacts.";
        
        // Identify question type and adjust response style
        let is_greeting = transcription.to_lowercase().contains("how are you") || 
                         transcription.to_lowercase().contains("good morning") ||
                         transcription.to_lowercase().contains("hello");

        let is_technical = transcription.to_lowercase().contains("react") ||
                          transcription.to_lowercase().contains("javascript") ||
                          transcription.to_lowercase().contains("frontend") ||
                          transcription.to_lowercase().contains("code") ||
                          transcription.to_lowercase().contains("programming");

        // Construct a context-aware prompt
        let prompt = if is_greeting {
            format!(
                r#"You are me in a frontend engineering job interview. This is a greeting/small talk question.

{transcription_note}

The interviewer says: "{question}"

Respond naturally but professionally. Keep it very brief and simple - just answer the greeting without volunteering too much information. Save the details about my background for when they actually ask about it."#,
                transcription_note = transcription_note,
                question = transcription
            )
        } else if is_first_question && !is_greeting {
            format!(
                r#"You are me in a frontend engineering job interview. Use this information about me to answer questions naturally:

{context}

{transcription_note}

The interviewer asks: "{question}"

Important guidelines:
1. Listen to the actual question - only answer what was asked
2. Be concise but specific when giving examples
3. Stay focused on the topic of the question
4. Use a natural, conversational tone
5. Don't volunteer information that wasn't asked for
6. If it's a technical question, show expertise but remain humble
7. If it's about my background, focus on relevant experience for the role
8. If the question has transcription artifacts, focus on the main intent"#,
                context = self.context,
                transcription_note = transcription_note,
                question = transcription
            )
        } else if is_technical {
            format!(
                r#"You are me in a frontend engineering job interview. Here's my background:

{context}

{transcription_note}

The interviewer asks this technical question: "{question}"

Guidelines for technical response:
1. Show practical experience, not just theoretical knowledge
2. Use specific examples from my work at Grupo SBF or previous roles
3. Demonstrate both technical depth and UX awareness
4. Be confident but not arrogant
5. Focus on real-world application and problem-solving
6. Keep the response focused and structured
7. If the question has transcription noise, address the core technical concept"#,
                context = self.context,
                transcription_note = transcription_note,
                question = transcription
            )
        } else {
            format!(
                r#"You are me in a frontend engineering job interview. You have my background:

{context}

{transcription_note}

The interviewer asks: "{question}"

Remember:
1. Only answer what was specifically asked
2. Use relevant examples from my experience
3. Keep the conversation natural and focused
4. Don't volunteer unrelated information
5. Be authentic but professional
6. If there's transcription noise, focus on the clear parts of the question"#,
                context = self.context,
                transcription_note = transcription_note,
                question = transcription
            )
        };

        let request = GeminiRequest {
            contents: vec![Content {
                parts: vec![Part {
                    text: prompt,
                }],
            }],
        };

        // Send request and get raw response first
        let response = client
            .post(&self.base_url)
            .query(&[("key", &self.api_key)])
            .json(&request)
            .send()
            .await?;

        // Get the response status and text
        let status = response.status();
        let response_text = response.text().await?;
        
        info!("API Response Status: {}", status);
        info!("API Response Body: {}", response_text);

        // Try to parse the response
        match serde_json::from_str::<GeminiResponse>(&response_text) {
            Ok(GeminiResponse::Success { candidates }) => {
                if let Some(candidate) = candidates.first() {
                    if let Some(part) = candidate.content.parts.first() {
                        // Clean up the response
                        let cleaned_response = part.text
                            .replace("[Key Points]", "")
                            .replace("[Response]", "")
                            .replace("Thank you for your question.", "")
                            .replace("That's a great question.", "")
                            .replace("Thank you for asking.", "")
                            .trim()
                            .to_string();
                        
                        info!("Successfully got response from Gemini");
                        return Ok(cleaned_response);
                    }
                }
                Ok("No response content available.".to_string())
            }
            Ok(GeminiResponse::Error { error }) => {
                error!("API Error: {} ({})", error.message, error.code);
                Ok(format!("Error: {}", error.message))
            }
            Err(e) => {
                error!("Failed to parse response: {}", e);
                Ok("Failed to process the response. Please try again.".to_string())
            }
        }
    }
} 