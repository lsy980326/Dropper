use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Stream {
    pub index: u32,
    #[serde(default)]
    pub codec_type: String,
    #[serde(default)]
    pub codec_name: String,
    #[serde(default)]
    pub codec_tag_string: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(default)]
    pub pix_fmt: String,
    #[serde(default)]
    pub field_order: String,
    #[serde(default)]
    pub color_primaries: String,
    #[serde(default)]
    pub color_transfer: String,
    #[serde(default)]
    pub color_space: String,
    pub bit_rate: Option<String>,
    pub duration: Option<String>,
    #[serde(default)]
    pub disposition: Value,
    #[serde(default)]
    pub tags: Value,
    #[serde(default)]
    pub side_data_list: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct Media {
    pub bytes: u64,
    pub duration: f64,
    pub streams: Vec<Stream>,
    pub video: Stream,
    pub hdr: Option<String>,
    pub blocked: Option<String>,
}

impl Media {
    pub fn from_probe(value: Value, bytes: u64) -> Result<Self, String> {
        if bytes == 0 {
            return Err("비어 있는 파일입니다.".into());
        }
        let streams: Vec<Stream> = serde_json::from_value(value["streams"].clone())
            .map_err(|_| "영상 트랙 정보를 읽을 수 없습니다.")?;
        let videos: Vec<_> = streams
            .iter()
            .filter(|s| {
                s.codec_type == "video" && s.disposition["attached_pic"].as_i64() != Some(1)
            })
            .collect();
        let video = videos
            .first()
            .ok_or("영상 트랙이 없는 파일입니다.")?
            .to_owned()
            .clone();
        let duration = value["format"]["duration"]
            .as_str()
            .and_then(|v| v.parse::<f64>().ok())
            .or_else(|| video.duration.as_ref().and_then(|v| v.parse().ok()))
            .filter(|v| v.is_finite() && *v > 0.0)
            .ok_or("재생 시간을 확인할 수 없습니다. 파일이 정상 재생되는지 확인해 주세요.")?;
        if video.width.unwrap_or(0) == 0 || video.height.unwrap_or(0) == 0 {
            return Err("영상 해상도를 확인할 수 없습니다.".into());
        }
        let side = serde_json::to_string(&video.side_data_list)
            .unwrap_or_default()
            .to_lowercase();
        let hdr = match video.color_transfer.as_str() {
            "smpte2084" => Some("HDR10".into()),
            "arib-std-b67" => Some("HLG".into()),
            _ => None,
        };
        let blocked = if side.contains("dovi")
            || side.contains("dolby")
            || video.codec_tag_string.starts_with("dv")
        {
            Some("Dolby Vision 메타데이터를 보존할 수 없어 이 파일은 건너뜁니다.".into())
        } else if side.contains("hdr10+")
            || side.contains("smpte2094")
            || side.contains("smpte 2094")
            || side.contains("dynamic hdr")
        {
            Some("HDR10+ 동적 메타데이터 보존이 지원되지 않습니다.".into())
        } else if videos.len() != 1 {
            Some("여러 영상 트랙이 있는 파일은 지원하지 않습니다. 오디오와 자막은 여러 트랙도 지원합니다.".into())
        } else if streams.iter().any(|s| {
            !matches!(
                s.codec_type.as_str(),
                "video" | "audio" | "subtitle" | "attachment"
            )
        }) {
            Some("보존 여부를 확인할 수 없는 데이터 트랙이 있습니다.".into())
        } else if !matches!(video.field_order.as_str(), "" | "unknown" | "progressive") {
            Some("인터레이스 영상은 지원하지 않습니다.".into())
        } else if ["yuva", "rgba", "argb", "bgra", "gbrap", "abgr"]
            .iter()
            .any(|s| video.pix_fmt.contains(s))
        {
            Some("투명도가 있는 영상은 원본 보존을 위해 건너뜁니다.".into())
        } else {
            None
        };
        Ok(Self {
            bytes,
            duration,
            streams,
            video,
            hdr,
            blocked,
        })
    }
    pub fn requires_mkv(&self) -> bool {
        self.streams.iter().any(|s| match s.codec_type.as_str() {
            "audio" => !matches!(
                s.codec_name.as_str(),
                "aac" | "mp3" | "ac3" | "eac3" | "alac"
            ),
            "subtitle" => s.codec_name != "mov_text",
            "attachment" => true,
            "video" => s.disposition["attached_pic"].as_i64() == Some(1),
            _ => false,
        })
    }
    pub fn audio_bps(&self) -> Option<u64> {
        self.streams
            .iter()
            .filter(|s| s.codec_type == "audio")
            .try_fold(0_u64, |sum, s| {
                let rate = s.bit_rate.as_ref()?.parse::<u64>().ok()?;
                sum.checked_add(rate)
            })
    }
}
