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

    pub async fn get_interview_response(&self, transcription: &str) -> Result<String, Box<dyn std::error::Error>> {
        info!("Getting interview response for transcription: {}", transcription);

        let client = reqwest::Client::new();
        
        // Construct a more focused prompt for quick responses
        let prompt = format!(
            r#"Based on this background:

{context}

Respond to this interview question: "{question}"

Requirements:
- Response should take 1 minute to speak
- Focus on design to frontend journey
- Include specific work examples
- Show both technical and UX skills

Format your response EXACTLY like this, without any introduction or extra text:

[Key Points]
• Point 1
• Point 2
• Point 3

[Response]
Your actual response here"#,
            context = self.context,
            question = transcription
        );

        info!("Sending request to Gemini API with prompt: {}", prompt);

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
                        info!("Successfully got response from Gemini");
                        return Ok(part.text.clone());
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