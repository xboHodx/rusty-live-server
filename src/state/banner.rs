//! # 题库管理模块
//!
//! 从 JSON 文件加载题库，生成随机的问题-答案对用于观众答题验证。
//!
//! ## 题目类型分布
//! - 日期问题（15%）：询问发布时间（年/月/日/时）
//! - 持续时间问题（15%）：询问公告/卡池持续时间
//! - 发布者问题（2%）：询问谁上传的
//! - 角色/游戏问题（58%）：最常见，询问角色或游戏名称
//! - 内容问题（10%）：询问公告内容

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use chrono::{Datelike, Timelike};

// ============================================================================
// 数据结构定义
// ============================================================================

/// 单条公告记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BannerAnnounce {
    /// 版本号（可选，支持字符串格式的数字如 "外·337"）
    #[serde(deserialize_with = "deserialize_optional_index")]
    pub revision: Option<u32>,
    /// 开始时间（格式：YYYY-MM-DD HH:MM:SS）
    pub start_time: String,
    /// 卡池持续时间（可选）
    #[serde(rename = "banner_life")]
    pub banner_life: Option<String>,
    /// 公告持续时间（可选）
    #[serde(rename = "announce_life")]
    pub announce_life: Option<String>,
    /// 公告内容
    pub content: String,
    /// 发布者名称
    pub publisher: String,
}

/// 反序列化可选的索引字段
///
/// 支持以下格式：
/// - null -> None
/// - 数字 -> Some(数字)
/// - 字符串（如 "外·337"）-> 提取数字部分
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
                // 提取字符串中的数字（处理 "外·337" 格式）
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

/// 单个卡池/公告条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Banner {
    /// 卡池序号（支持字符串格式的数字如 "外·337"）
    #[serde(deserialize_with = "deserialize_index")]
    pub index: u32,
    /// 游戏名称（可选）
    #[serde(default)]
    pub game: Option<String>,
    /// 角色名称（可选）
    #[serde(default)]
    pub character: Option<String>,
    /// 公告列表
    pub announces: Vec<BannerAnnounce>,
}

/// 反序列化索引字段
///
/// 与 `deserialize_optional_index` 类似，但返回必选值
fn deserialize_index<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(n) => {
            // 尝试解析为 u64，失败则尝试 f64 并截断
            if let Some(v) = n.as_u64() {
                Ok(v as u32)
            } else if let Some(f) = n.as_f64() {
                Ok(f as u32)
            } else {
                Err(serde::de::Error::custom("invalid number"))
            }
        }
        serde_json::Value::String(s) => {
            // 提取字符串中的数字（处理 "外·337" 格式）
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

/// 题库数据库
///
/// 从 JSON 文件加载卡池数据，生成随机问题
pub struct BannerDatabase {
    /// 卡池列表
    banners: Vec<Banner>,
}

/// 问题-答案对
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BannerQuestion {
    /// 问题文本
    pub question: String,
    /// 答案文本
    pub answer: String,
}

impl BannerDatabase {
    /// 从 JSON 文件创建题库
    ///
    /// ### 参数
    /// - `path`: JSON 文件路径
    ///
    /// ### JSON 格式
    /// ```json
    /// [
    ///   {
    ///     "index": 337,
    ///     "game": "原神",
    ///     "character": "胡桃",
    ///     "announces": [...]
    ///   }
    /// ]
    /// ```
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let banners: Vec<Banner> = serde_json::from_str(&content)?;
        Ok(Self { banners })
    }

    /// 获取随机问题-答案对
    ///
    /// ### 返回值
    /// 返回 (问题, 答案) 元组
    ///
    /// ### 题目类型分布
    /// - 0-15: 日期问题（15%）
    /// - 15-30: 持续时间问题（15%）
    /// - 30-32: 发布者问题（2%）
    /// - 32-90: 角色/游戏问题（58%）
    /// - 90-100: 内容问题（10%）
    pub fn random_question(&self) -> (String, String) {
        if self.banners.is_empty() {
            return (
                "No questions available".to_string(),
                "N/A".to_string(),
            );
        }

        let mut rng = rand::thread_rng();

        // 加权随机选择题目类型（总和 100）
        let question_type = rng.gen_range(0..100);

        // 随机选择一个卡池（跳过索引 0，因为可能是占位符）
        let idx = rng.gen_range(1..self.banners.len()).max(1);

        // 根据权重分发到不同的问题生成函数
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

    /// 生成日期问题（15%）
    ///
    /// 询问卡池的发布时间（年/月/日/时）
    fn date_question(&self, idx: usize) -> (String, String) {
        let banner = &self.banners[idx];
        let mut rng = rand::thread_rng();

        // 随机选择一条公告
        let announce_idx = rng.gen_range(0..banner.announces.len());
        let announce = &banner.announces[announce_idx];

        // 解析时间字符串
        let date_formats = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d"];
        let parsed = date_formats
            .iter()
            .find_map(|fmt| chrono::NaiveDateTime::parse_from_str(&announce.start_time, fmt).ok());

        let (suffix, answer) = if let Some(dt) = parsed {
            // 可选的时间字段：年、月、日、时
            let keys = [
                ("tm_year", dt.year(), "是哪一年发布的?"),
                ("tm_mon", dt.month() as i32, "是哪一月发布的?"),
                ("tm_mday", dt.day() as i32, "是该月几号发布的?"),
                ("tm_hour", dt.hour() as i32, "是当天几点发布的(精确到小时)?"),
            ];
            let (key, val, suffix) = keys[rng.gen_range(0..keys.len())];
            (format!("的第{}篇公告", announce.revision.unwrap_or(1)), val.to_string())
        } else {
            // 解析失败，使用默认值
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

    /// 生成持续时间问题（15%）
    ///
    /// 询问卡池或公告的持续时间
    fn life_question(&self, idx: usize) -> (String, String) {
        let banner = &self.banners[idx];
        let mut rng = rand::thread_rng();

        // 随机选择：卡池持续时间 或 公告持续时间
        let (answer, suffix) = if banner.announces.len() == 1 || rng.gen_range(0..2) == 0 {
            // 卡池持续时间
            if let Some(ref life) = banner.announces[0].banner_life {
                (life.clone(), String::new())
            } else {
                ("7天".to_string(), String::new())
            }
        } else {
            // 公告持续时间
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

        // 提取数值部分（去掉 "天"、"小时" 等单位）
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

    /// 生成发布者问题（2%）
    ///
    /// 询问谁上传了该卡池
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

    /// 生成角色/游戏问题（58%）
    ///
    /// 最常见的问题类型，询问角色对应的游戏或游戏对应的角色
    fn character_game_question(&self, idx: usize) -> (String, String) {
        let banner = &self.banners[idx];
        let mut rng = rand::thread_rng();

        // 如果游戏或角色信息缺失，回退到发布者问题
        let (game, character) = match (&banner.game, &banner.character) {
            (Some(g), Some(c)) => (g, c),
            _ => {
                return self.publisher_question(idx);
            }
        };

        // 随机选择：问角色 还是 问游戏
        let mode = rng.gen_range(0..2);

        if mode == 0 {
            // 问：某游戏里的哪个角色？
            (
                format!(
                    "{}期公告娘是游戏{}里的哪个角色？",
                    banner.index, game
                ),
                character.to_string(),
            )
        } else {
            // 问：某角色是哪个游戏里的？
            (
                format!(
                    "{}期公告娘{}是哪个游戏里的角色？",
                    banner.index, character
                ),
                game.to_string(),
            )
        }
    }

    /// 生成内容问题（10%）
    ///
    /// 询问公告内容（首行或第 N 个中文字符）
    fn content_question(&self, idx: usize) -> (String, String) {
        let banner = &self.banners[idx];
        let mut rng = rand::thread_rng();

        let announce_idx = rng.gen_range(0..banner.announces.len());
        let announce = &banner.announces[announce_idx];

        // 构建后缀（如果有多个公告）
        let suffix = if banner.announces.len() > 1 {
            format!("的第{}篇公告", announce.revision.unwrap_or(1))
        } else {
            String::new()
        };

        // 随机选择：首行问题 或 第 N 个字问题
        let mode = rng.gen_range(0..10);

        if mode > 1 {
            // 首行问题（约 90% 概率）
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
            // 第 N 个中文字符问题（约 10% 概率）
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

/// 判断字符是否为中文字符
///
/// ### 检测范围
/// - CJK 统一汉字（U+4E00 - U+9FFF）
/// - CJK 扩展 A-F
/// - CJK 符号和标点
/// - 全角字符
fn is_chinese(c: char) -> bool {
    match c {
        '\u{4E00}'..='\u{9FFF}' => true,  // CJK 统一汉字
        '\u{3400}'..='\u{4DBF}' => true,  // CJK 扩展 A
        '\u{20000}'..='\u{2A6DF}' => true, // CJK 扩展 B
        '\u{2A700}'..='\u{2B73F}' => true, // CJK 扩展 C
        '\u{2B740}'..='\u{2B81F}' => true, // CJK 扩展 D
        '\u{2B820}'..='\u{2CEAF}' => true, // CJK 扩展 E
        '\u{2CEB0}'..='\u{2EBEF}' => true, // CJK 扩展 F
        '\u{3000}'..='\u{303F}' => true,  // CJK 符号和标点
        '\u{FF00}'..='\u{FFEF}' => true,  // 全角字符
        _ => false,
    }
}
