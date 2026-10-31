//! 명세서 텍스트를 청구항 · 도면의 간단한 설명 · 본문 · 첨부 도면으로 나눈다.

use regex::Regex;
use std::sync::LazyLock;

/// 도면 번호. (1, "a") = 도 1a
pub type Fig = (u32, String);

pub struct Claim {
    pub num: u32,
    pub text: String,
    /// 인용하는 청구항 번호 (범위 펼침 · 정렬 · 중복 제거)
    pub refs: Vec<u32>,
}

#[derive(Default)]
pub struct Spec {
    pub claims: Vec<Claim>,
    pub brief: String,
    pub body: String,
    /// 【부호의 설명】 ("10: 기판 20: 전극")
    pub signs: String,
    /// 첨부 도면의 【도 N】 목록. None = 도면 섹션이 아예 없음
    pub drawings: Option<Vec<Fig>>,
}

enum Sec {
    Body,
    Claim,
    Brief,
    Signs,
    Skip,
}

static HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*[【\[]([^】\]]+)[】\]]\s*").unwrap());
static CLAIM_REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"제\s*(\d+)\s*항|청구항\s*(\d+)").unwrap());
static FIG_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)(?:\(([A-Za-z])\)|([A-Za-z]))?$").unwrap());

const BODY_HEADS: &[&str] = &[
    "발명", "기술분야", "배경기술", "선행기술", "해결하려는과제", "과제의해결", "실시예",
    "산업상이용", "명세서", "서열목록",
];

pub fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// "1", "1a", "1(a)" → Fig
pub fn fig_id(s: &str) -> Option<Fig> {
    let c = FIG_ID.captures(s)?;
    let suffix = c.get(2).or(c.get(3)).map_or(String::new(), |m| m.as_str().to_lowercase());
    Some((c[1].parse().ok()?, suffix))
}

pub fn parse(text: &str) -> Spec {
    let mut spec = Spec::default();
    let mut sec = Sec::Body;
    for line in text.lines() {
        let mut rest = line;
        if let Some(m) = HEADER.captures(line) {
            let name = squash(&m[1]);
            rest = &line[m.get(0).unwrap().end()..];
            if let Some(n) = name.strip_prefix("청구항").and_then(|n| n.parse().ok()) {
                sec = Sec::Claim;
                spec.claims.push(Claim { num: n, text: String::new(), refs: vec![] });
            } else if name == "도면의간단한설명" {
                sec = Sec::Brief;
            } else if name == "부호의설명" {
                sec = Sec::Signs;
            } else if name == "도면" {
                sec = Sec::Skip;
                spec.drawings.get_or_insert_with(Vec::new);
            } else if let Some(f) = name.strip_prefix('도').and_then(fig_id) {
                sec = Sec::Skip;
                spec.drawings.get_or_insert_with(Vec::new).push(f);
            } else if name.contains("요약") || name == "대표도" || name.ends_with("청구범위") {
                sec = Sec::Skip;
            } else if BODY_HEADS.iter().any(|h| name.starts_with(h)) {
                sec = Sec::Body;
            }
            // 그 밖의 머리말([0001], 【표 1】, 【수학식 1】)은 섹션을 바꾸지 않는다
        }
        let buf = match sec {
            Sec::Body => &mut spec.body,
            Sec::Brief => &mut spec.brief,
            Sec::Signs => &mut spec.signs,
            Sec::Claim => &mut spec.claims.last_mut().unwrap().text,
            Sec::Skip => continue,
        };
        buf.push_str(rest);
        buf.push('\n');
    }
    for c in &mut spec.claims {
        c.refs = claim_refs(&c.text);
    }
    spec
}

/// "제1항 내지 제3항" → [1, 2, 3], "청구항 1 또는 청구항 4" → [1, 4]
fn claim_refs(text: &str) -> Vec<u32> {
    let mut refs = vec![];
    let mut prev: Option<(usize, u32)> = None;
    for c in CLAIM_REF.captures_iter(text) {
        let m = c.get(0).unwrap();
        let n: u32 = c.get(1).or(c.get(2)).unwrap().as_str().parse().unwrap_or(0);
        if let Some((end, p)) = prev {
            let between = text[end..m.start()].trim();
            // shortcut: 범위는 100개까지만 펼친다, 그보다 큰 청구범위가 생기면 늘릴 것
            if (between == "내지" || between == "~") && p < n && n - p <= 100 {
                refs.extend(p + 1..n);
            }
        }
        refs.push(n);
        prev = Some((m.end(), n));
    }
    refs.sort();
    refs.dedup();
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_ranges_and_lists() {
        assert_eq!(claim_refs("제1항 내지 제3항 중 어느 한 항에 있어서"), vec![1, 2, 3]);
        assert_eq!(claim_refs("청구항 1 또는 청구항 4에 있어서"), vec![1, 4]);
        assert_eq!(claim_refs("제 2 항에 있어서"), vec![2]);
    }

    #[test]
    fn fig_ids() {
        assert_eq!(fig_id("1"), Some((1, String::new())));
        assert_eq!(fig_id("2A"), Some((2, "a".into())));
        assert_eq!(fig_id("3(b)"), Some((3, "b".into())));
        assert_eq!(fig_id("면"), None);
    }
}
