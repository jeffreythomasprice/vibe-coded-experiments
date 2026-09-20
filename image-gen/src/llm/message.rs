use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::llm::LlmError;
use crate::llm::tool::ToolCall;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Png,
    Jpeg,
    Webp,
}

impl MediaType {
    pub fn as_mime(self) -> &'static str {
        match self {
            MediaType::Png => "image/png",
            MediaType::Jpeg => "image/jpeg",
            MediaType::Webp => "image/webp",
        }
    }

    /// Sniffs the type from magic bytes rather than a file extension, since a
    /// mislabeled extension would otherwise silently send the wrong MIME type.
    fn sniff(data: &[u8]) -> Option<MediaType> {
        if data.starts_with(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']) {
            Some(MediaType::Png)
        } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(MediaType::Jpeg)
        } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
            Some(MediaType::Webp)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub media_type: MediaType,
    pub data: Vec<u8>,
}

impl Image {
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, LlmError> {
        let media_type = MediaType::sniff(&data).ok_or(LlmError::UnsupportedImageType { path: None })?;
        Ok(Self { media_type, data })
    }

    pub fn from_path(path: &std::path::Path) -> Result<Self, LlmError> {
        let data = std::fs::read(path).map_err(|source| LlmError::ImageRead {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_bytes(data).map_err(|_| LlmError::UnsupportedImageType {
            path: Some(path.to_path_buf()),
        })
    }

    pub fn to_base64(&self) -> String {
        BASE64.encode(&self.data)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Text(String),
    Image(Image),
}

impl Content {
    pub fn text(text: impl Into<String>) -> Self {
        Content::Text(text.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Content>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
}

impl Message {
    fn bare(role: Role, content: Vec<Content>) -> Self {
        Self {
            role,
            content,
            tool_calls: Vec::new(),
            tool_call_id: None,
            tool_name: None,
        }
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self::bare(Role::System, vec![Content::text(text)])
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self::bare(Role::User, vec![Content::text(text)])
    }

    pub fn user_with_images(text: impl Into<String>, images: Vec<Image>) -> Self {
        let mut content = vec![Content::text(text)];
        content.extend(images.into_iter().map(Content::Image));
        Self::bare(Role::User, content)
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self::bare(Role::Assistant, vec![Content::text(text)])
    }

    pub fn assistant_with_tool_calls(text: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            tool_calls,
            ..Self::bare(Role::Assistant, vec![Content::text(text)])
        }
    }

    pub fn tool_result(call: &ToolCall, content: Vec<Content>) -> Self {
        Self {
            tool_call_id: Some(call.id.clone()),
            tool_name: Some(call.name.clone()),
            ..Self::bare(Role::Tool, content)
        }
    }

    /// Concatenates every text part; used by providers whose wire format has a
    /// single `content` string rather than our `Vec<Content>`.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|part| match part {
                Content::Text(text) => Some(text.as_str()),
                Content::Image(_) => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn images(&self) -> impl Iterator<Item = &Image> {
        self.content.iter().filter_map(|part| match part {
            Content::Image(image) => Some(image),
            Content::Text(_) => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

    #[test]
    fn sniffs_png_from_magic_bytes() {
        let mut data = PNG_MAGIC.to_vec();
        data.extend_from_slice(b"rest of file");
        let image = Image::from_bytes(data).unwrap();
        assert_eq!(image.media_type, MediaType::Png);
    }

    #[test]
    fn sniffs_jpeg_from_magic_bytes() {
        let data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 0];
        let image = Image::from_bytes(data).unwrap();
        assert_eq!(image.media_type, MediaType::Jpeg);
    }

    #[test]
    fn sniffs_webp_from_riff_container() {
        let mut data = b"RIFF".to_vec();
        data.extend_from_slice(&[0, 0, 0, 0]);
        data.extend_from_slice(b"WEBP");
        let image = Image::from_bytes(data).unwrap();
        assert_eq!(image.media_type, MediaType::Webp);
    }

    #[test]
    fn unrecognized_bytes_are_rejected() {
        let err = Image::from_bytes(b"not an image".to_vec()).unwrap_err();
        assert!(matches!(err, LlmError::UnsupportedImageType { path: None }));
    }

    #[test]
    fn base64_round_trips() {
        let mut data = PNG_MAGIC.to_vec();
        data.extend_from_slice(&[1, 2, 3, 4, 5]);
        let image = Image::from_bytes(data.clone()).unwrap();
        let decoded = BASE64.decode(image.to_base64()).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn message_text_joins_only_text_parts() {
        let png = Image {
            media_type: MediaType::Png,
            data: vec![],
        };
        let msg = Message::user_with_images("describe this", vec![png]);
        assert_eq!(msg.text(), "describe this");
        assert_eq!(msg.images().count(), 1);
    }
}
