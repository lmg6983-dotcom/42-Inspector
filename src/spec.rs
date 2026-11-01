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

/// 줄이 속한 섹션. `parse`와 문체 검사(`checks::style`)가 같은 판단을 쓴다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    Body,
    Claim,
    Brief,
    Signs,
    /// 요약서 · 요약 · 대표도 (검사에서 빼지만 문체는 본다)
    Abstract,
    /// 청구범위 머리말 줄, 첨부 도면
    Skip,
}

/// 한 줄을 나눈 결과: 섹션, 줄 앞 머리말(띄어쓰기 없앤 이름), 머리말 뒤 본문
pub struct Line<'a> {
    pub part: Part,
    pub header: Option<String>,
    pub rest: &'a str,
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

/// 줄마다 섹션을 정한다. 【표 1】 · 【수학식 1】 · [0001] 같은 머리말은 섹션을 바꾸지 않는다.
pub fn lines(text: &str) -> Vec<Line<'_>> {
    let mut part = Part::Body;
    let mut out = vec![];
    for line in text.lines() {
        let mut rest = line;
        let mut header = None;
        if let Some(m) = HEADER.captures(line) {
            let name = squash(&m[1]);
            rest = &line[m.get(0).unwrap().end()..];
            if name.strip_prefix("청구항").is_some_and(|n| n.parse::<u32>().is_ok()) {
                part = Part::Claim;
            } else if name == "도면의간단한설명" {
                part = Part::Brief;
            } else if name == "부호의설명" {
                part = Part::Signs;
            } else if name == "도면" || name.strip_prefix('도').and_then(fig_id).is_some() || name.ends_with("청구범위") {
                part = Part::Skip;
            } else if name.contains("요약") || name == "대표도" {
                part = Part::Abstract;
            } else if BODY_HEADS.iter().any(|h| name.starts_with(h)) {
                part = Part::Body;
            }
            header = Some(name);
        }
        out.push(Line { part, header, rest });
    }
    out
}

pub fn parse(text: &str) -> Spec {
    let mut spec = Spec::default();
    for l in lines(text) {
        let h = l.header.as_deref().unwrap_or("");
        if l.part == Part::Claim {
            if let Some(n) = h.strip_prefix("청구항").and_then(|n| n.parse().ok()) {
                spec.claims.push(Claim { num: n, text: String::new(), refs: vec![] });
            }
        } else if l.part == Part::Skip {
            if h == "도면" {
                spec.drawings.get_or_insert_with(Vec::new);
            } else if let Some(f) = h.strip_prefix('도').and_then(fig_id) {
                spec.drawings.get_or_insert_with(Vec::new).push(f);
            }
        }
        let buf = match l.part {
            Part::Body => &mut spec.body,
            Part::Brief => &mut spec.brief,
            Part::Signs => &mut spec.signs,
            Part::Claim => &mut spec.claims.last_mut().unwrap().text,
            Part::Abstract | Part::Skip => continue,
        };
        buf.push_str(l.rest);
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
