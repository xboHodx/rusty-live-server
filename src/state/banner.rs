use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use chrono::{Datelike, Timelike};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BannerAnnounce {
    #[serde(deserialize_with = "deserialize_optional_index")]
    pub revision: Option<u32>,
    pub start_time: String,
    #[serde(rename = "banner_life")]
    pub banner_life: Option<String>,
    #[serde(rename = "announce_life")]
    pub announce_life: Option<String>,
    pub content: String,
    pub publisher: String,
}

fn deserialize_optional_index<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Number(n) => {
            if let Some(v) = n.as_u64() {
                Ok(Some(v as u32))
            } else if let Some(f) = n.as_f64() {
                Ok(Some(f as u32))
            } else {
                Err(serde::de::Error::custom("invalid number"))
            }
        }
        serde_json::Value::String(s) => {
            if s.is_empty() {
                Ok(None)
            } else {
                // Extract digits from string (handles "外·337" format)
                let digits: String = s.chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect();

                if digits.is_empty() {
                    Err(serde::de::Error::custom("no digits found"))
                } else {
                    digits.parse::<u32>().map(Some).map_err(|_| {
                        serde::de::Error::custom("invalid string number")
                    })
                }
            }
        }
        _ => Err(serde::de::Error::custom("invalid type")),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Banner {
    #[serde(deserialize_with = "deserialize_index")]
    pub index: u32,
    #[serde(default)]
    pub game: Option<String>,
    #[serde(default)]
    pub character: Option<String>,
    pub announces: Vec<BannerAnnounce>,
}

fn deserialize_index<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(n) => {
            // Try as u64 first, then as f64 and truncate
            if let Some(v) = n.as_u64() {
                Ok(v as u32)
            } else if let Some(f) = n.as_f64() {
                Ok(f as u32)
            } else {
                Err(serde::de::Error::custom("invalid number"))
            }
        }
        serde_json::Value::String(s) => {
            // Extract digits from string (handles "外·337" format)
            let digits: String = s.chars()
                .filter(|c| c.is_ascii_digit())
                .collect();

            if digits.is_empty() {
                Err(serde::de::Error::custom("no digits found"))
            } else {
                digits.parse::<u32>().map_err(|_| {
                    serde::de::Error::custom("invalid string number")
                })
            }
        }
        _ => Err(serde::de::Error::custom("invalid type")),
    }
}

/// Banner database for generating random questions
pub struct BannerDatabase {
    banners: Vec<Banner>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BannerQuestion {
    pub question: String,
    pub answer: String,
}

impl BannerDatabase {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let banners: Vec<Banner> = serde_json::from_str(&content)?;
        Ok(Self { banners })
    }

    /// Get a random question-answer pair
    pub fn random_question(&self) -> (String, String) {
        if self.banners.is_empty() {
            return (
                "No questions available".to_string(),
                "N/A".to_string(),
            );
        }

        let mut rng = rand::thread_rng();

        // Weighted question type selection (100 total)
        let question_type = rng.gen_range(0..100);

        // Ensure we have at least one banner (index 0 is dummy in original)
        let idx = rng.gen_range(1..self.banners.len()).max(1);

        // Weight distribution matches Python version:
        // date_question: 15%, life_question: 15%, publisher_question: 2%,
        // character_game_question: 58%, content_question: 10%
        if question_type < 15 {
            self.date_question(idx)
        } else if question_type < 30 {
            self.life_question(idx)
        } else if question_type < 32 {
            self.publisher_question(idx)
        } else if question_type < 90 {
            self.character_game_question(idx)
        } else {
            self.content_question(idx)
        }
    }

    /// Date question (15%) - ask about release date
    fn date_question(&self, idx: usize) -> (String, String) {
        let banner = &self.banners[idx];
        let mut rng = rand::thread_rng();

        let announce_idx = rng.gen_range(0..banner.announces.len());
        let announce = &banner.announces[announce_idx];

        // Parse the time string
        let date_formats = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d"];
        let parsed = date_formats
            .iter()
            .find_map(|fmt| chrono::NaiveDateTime::parse_from_str(&announce.start_time, fmt).ok());

        let (suffix, answer) = if let Some(dt) = parsed {
            let keys = [
                ("tm_year", dt.year(), "是哪一年发布的?"),
                ("tm_mon", dt.month() as i32, "是哪一月发布的?"),
                ("tm_mday", dt.day() as i32, "是该月几号发布的?"),
                ("tm_hour", dt.hour() as i32, "是当天几点发布的(精确到小时)?"),
            ];
            let (key, val, suffix) = keys[rng.gen_range(0..keys.len())];
            (format!("的第{}篇公告", announce.revision.unwrap_or(1)), val.to_string())
        } else {
            ("发布".to_string(), "2024".to_string())
        };

        if banner.announces.len() == 1 {
            (format!("{}期公告娘{}", banner.index, suffix), answer)
        } else {
            (
                format!("{}期公告娘{}", banner.index, suffix),
                answer,
            )
        }
    }

    /// Life question (15%) - ask about duration
    fn life_question(&self, idx: usize) -> (String, String) {
        let banner = &self.banners[idx];
        let mut rng = rand::thread_rng();

        let (answer, suffix) = if banner.announces.len() == 1 || rng.gen_range(0..2) == 0 {
            // Banner life
            if let Some(ref life) = banner.announces[0].banner_life {
                (life.clone(), String::new())
            } else {
                ("7天".to_string(), String::new())
            }
        } else {
            // Announce life
            let announce_idx = rng.gen_range(0..banner.announces.len());
            let announce = &banner.announces[announce_idx];
            if let Some(ref life) = announce.announce_life {
                (
                    life.clone(),
                    format!("的第{}篇公告", announce.revision.unwrap_or(1)),
                )
            } else {
                (
                    "7天".to_string(),
                    format!("的第{}篇公告", announce.revision.unwrap_or(1)),
                )
            }
        };

        let unit = if answer.contains("天") { "天" } else { "小时" };
        let numeric_answer = answer
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>();

        (
            format!("{}期公告娘{}持续了几{}?", banner.index, suffix, unit),
            numeric_answer,
        )
    }

    /// Publisher question (2%) - who uploaded
    fn publisher_question(&self, idx: usize) -> (String, String) {
        let banner = &self.banners[idx];

        if banner.announces.len() == 1 {
            (
                format!("{}期公告娘是谁上传的?", banner.index),
                banner.announces[0].publisher.clone(),
            )
        } else {
            let mut rng = rand::thread_rng();
            let announce_idx = rng.gen_range(0..banner.announces.len());
            let announce = &banner.announces[announce_idx];
            (
                format!(
                    "{}期公告娘的第{}篇公告是谁上传的?",
                    banner.index,
                    announce.revision.unwrap_or(1)
                ),
                announce.publisher.clone(),
            )
        }
    }

    /// Character/Game question (58%) - most common
    fn character_game_question(&self, idx: usize) -> (String, String) {
        let banner = &self.banners[idx];
        let mut rng = rand::thread_rng();

        // Skip if game or character is null
        let (game, character) = match (&banner.game, &banner.character) {
            (Some(g), Some(c)) => (g, c),
            _ => {
                // Fall back to publisher question
                return self.publisher_question(idx);
            }
        };

        let mode = rng.gen_range(0..2);

        if mode == 0 {
            (
                format!(
                    "{}期公告娘是游戏{}里的哪个角色？",
                    banner.index, game
                ),
                character.to_string(),
            )
        } else {
            (
                format!(
                    "{}期公告娘{}是哪个游戏里的角色？",
                    banner.index, character
                ),
                game.to_string(),
            )
        }
    }

    /// Content question (10%) - ask about content
    fn content_question(&self, idx: usize) -> (String, String) {
        let banner = &self.banners[idx];
        let mut rng = rand::thread_rng();

        let announce_idx = rng.gen_range(0..banner.announces.len());
        let announce = &banner.announces[announce_idx];

        let suffix = if banner.announces.len() > 1 {
            format!("的第{}篇公告", announce.revision.unwrap_or(1))
        } else {
            String::new()
        };

        // Mode 0: specific character, Mode 1-9: first line
        let mode = rng.gen_range(0..10);

        if mode > 1 {
            // First line question
            let answer = announce
                .content
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            (
                format!("{}期公告娘{}的内容的第一个换行或空格之前的内容是什么？", banner.index, suffix),
                answer,
            )
        } else {
            // Nth character question (Chinese characters only)
            let chinese_chars: Vec<char> = announce
                .content
                .chars()
                .filter(|c| is_chinese(*c))
                .collect();

            if chinese_chars.is_empty() {
                (
                    format!("{}期公告娘{}的内容是什么？", banner.index, suffix),
                    announce.content.clone(),
                )
            } else {
                let ch_idx = rng.gen_range(0..chinese_chars.len());
                (
                    format!(
                        "{}期公告娘{}的内容的第{}个字是什么(不包含英文字符和半角标点符号)？",
                        banner.index,
                        suffix,
                        ch_idx + 1
                    ),
                    chinese_chars[ch_idx].to_string(),
                )
            }
        }
    }
}

/// Check if a character is Chinese (CJK Unified Ideographs)
fn is_chinese(c: char) -> bool {
    match c {
        '\u{4E00}'..='\u{9FFF}' => true,  // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}' => true,  // CJK Unified Ideographs Extension A
        '\u{20000}'..='\u{2A6DF}' => true, // CJK Unified Ideographs Extension B
        '\u{2A700}'..='\u{2B73F}' => true, // CJK Unified Ideographs Extension C
        '\u{2B740}'..='\u{2B81F}' => true, // CJK Unified Ideographs Extension D
        '\u{2B820}'..='\u{2CEAF}' => true, // CJK Unified Ideographs Extension E
        '\u{2CEB0}'..='\u{2EBEF}' => true, // CJK Unified Ideographs Extension F
        '\u{3000}'..='\u{303F}' => true,  // CJK Symbols and Punctuation
        '\u{FF00}'..='\u{FFEF}' => true,  // Halfwidth and Fullwidth Forms
        _ => false,
    }
}
