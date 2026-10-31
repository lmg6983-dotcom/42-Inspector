//! 검사 항목. 각 함수는 참고 정보(info)와 지적 사항(problems)을 돌려준다.

use crate::spec::{Fig, Spec, squash};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

pub struct Report {
    pub title: &'static str,
    pub info: Vec<String>,
    pub problems: Vec<String>,
}

impl Report {
    fn new(title: &'static str) -> Self {
        Report { title, info: vec![], problems: vec![] }
    }
}

// ───────────── 인용항 ─────────────

pub fn citations(spec: &Spec) -> Report {
    let mut r = Report::new("인용항 체크");
    if spec.claims.is_empty() {
        r.info.push("【청구항 N】 머리말을 찾지 못해 건너뜀".into());
        return r;
    }
    let find = |n: u32| spec.claims.iter().find(|c| c.num == n);
    let indep = spec.claims.iter().filter(|c| c.refs.is_empty()).count();
    r.info.push(format!("청구항 {}개 (독립항 {indep}개)", spec.claims.len()));

    let mut prev = 0;
    for c in &spec.claims {
        if c.num != prev + 1 {
            r.problems.push(format!("청구항 번호가 {prev} 다음에 {}로 이어짐 (누락 · 중복 확인)", c.num));
        }
        prev = c.num;
        for &n in &c.refs {
            if n >= c.num {
                r.problems.push(format!("청구항 {}: 자기 자신이나 뒤의 청구항 {n}을 인용", c.num));
            } else if find(n).is_none() {
                r.problems.push(format!("청구항 {}: 없는 청구항 {n}을 인용", c.num));
            }
        }
        if c.refs.len() < 2 {
            continue;
        }
        // 특허법 시행령 제5조 제5항: 2 이상 인용 시 택일적으로 기재
        if !preamble(&c.text).contains("또는") && !preamble(&c.text).contains("어느") {
            r.problems.push(format!("청구항 {}: 2 이상의 항을 택일적(또는 / 중 어느 한 항)으로 인용하지 않음", c.num));
        }
        // 같은 조 제6항: 다중인용항은 다른 다중인용항을 인용할 수 없음
        for &n in &c.refs {
            if find(n).is_some_and(|p| p.refs.len() > 1) {
                r.problems.push(format!("청구항 {}: 다중인용항이 다중인용항(청구항 {n})을 인용", c.num));
            }
        }
    }
    r
}

/// 인용 부분: 처음부터 첫 쉼표까지 ("제1항 또는 제2항에 있어서,")
fn preamble(text: &str) -> &str {
    &text[..text.find(',').unwrap_or(text.len())]
}

// ───────────── 상기 ─────────────

static ORDINAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^제\d+$").unwrap());

/// 긴 것부터. 명사 끝 글자와 겹쳐도("회로" → "회") 원형부터 찾으므로 문제없다.
const JOSA: &[&str] = &[
    "으로부터", "에있어서", "로부터", "에서의", "에서는", "으로서", "으로써", "으로의", "에대한",
    "에서", "으로", "에게", "에의", "에는", "에도", "과의", "와의", "과는", "와는", "이며", "이고",
    "이다", "이나", "보다", "까지", "처럼", "로서", "로써", "로", "에", "의", "은", "는", "이", "가",
    "을", "를", "과", "와", "도", "만",
];

/// "상기 제1 전극(10)과 접촉" → ["제1전극"], "상기 절연층과" → ["절연층과", "절연층"]
/// [0]은 원형, [1]은 가장 긴 조사를 뗀 어간.
fn term_candidates(rest: &str) -> Vec<String> {
    let mut toks = rest.split_whitespace();
    let Some(first) = toks.next() else { return vec![] };
    let mut t = first.to_string();
    if ORDINAL.is_match(first) {
        t.extend(toks.next());
    }
    if let Some(i) = t.find('(') {
        t.truncate(i);
    }
    let t = t.trim_end_matches(|c: char| !c.is_alphanumeric()).to_string();
    if t.is_empty() {
        return vec![];
    }
    let mut out = vec![t.clone()];
    for j in JOSA {
        if let Some(stem) = t.strip_suffix(j).filter(|s| !s.is_empty()) {
            out.push(stem.to_string());
        }
    }
    out
}

pub fn sanggi(spec: &Spec) -> Report {
    let mut r = Report::new("상기 체크");
    // 청구항 머리말이 없으면 붙여 넣은 글 전체를 한 덩어리로 본다
    let blocks: Vec<(u32, &str)> = if spec.claims.is_empty() {
        vec![(0, spec.body.as_str())]
    } else {
        spec.claims.iter().map(|c| (c.num, c.text.as_str())).collect()
    };

    let mut checked = 0;
    for &(num, text) in &blocks {
        let mut anc = BTreeSet::new();
        ancestors(spec, num, &mut anc);
        let cited: String = anc.iter().filter_map(|n| spec.claims.iter().find(|c| c.num == *n)).map(|c| squash(&c.text)).collect();
        let mut missing: Vec<String> = vec![];
        for (p, _) in text.match_indices("상기") {
            let before = text[..p].chars().next_back();
            let after = text[p + "상기".len()..].chars().next();
            if before.is_some_and(|c| c.is_alphanumeric()) || !after.is_some_and(char::is_whitespace) {
                continue; // "상기한", "상기와" 등
            }
            let cands = term_candidates(&text[p + "상기".len()..]);
            let Some(stem) = cands.get(1).or(cands.first()).cloned() else { continue };
            checked += 1;
            let earlier = squash(&text[..p]);
            if !cands.iter().any(|t| earlier.contains(t.as_str()) || cited.contains(t.as_str())) && !missing.contains(&stem) {
                missing.push(stem);
            }
        }
        let who = if num == 0 { "본문".to_string() } else { format!("청구항 {num}") };
        for m in missing {
            r.problems.push(format!("{who}: 상기 {m} — 앞에서(인용항 포함) 언급 안 됨"));
        }
    }
    r.info.push(format!("'상기' {checked}곳 확인"));
    r
}

fn ancestors(spec: &Spec, num: u32, out: &mut BTreeSet<u32>) {
    let Some(c) = spec.claims.iter().find(|c| c.num == num) else { return };
    for &n in &c.refs {
        // n < num: 순환 인용이 있어도 끝나도록
        if n < num && out.insert(n) {
            ancestors(spec, n, out);
        }
    }
}

// ───────────── 부재번호 ─────────────

static NUMERAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"((?:제\d+\s?)?[\p{Hangul}A-Za-z][\p{Hangul}A-Za-z0-9]*)\s?\(([A-Za-z]?\d+[A-Za-z0-9'-]*)\)").unwrap()
});

/// "S110" → ("S", 110, ""), "10a" → ("", 10, "a"): 사람이 읽는 순서로 정렬
fn natural(s: &str) -> (String, u32, String) {
    let i = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    let j = s[i..].find(|c: char| !c.is_ascii_digit()).map_or(s.len(), |k| i + k);
    (s[..i].to_string(), s[i..j].parse().unwrap_or(0), s[j..].to_string())
}

/// 정렬 키 → (원래 부호, 이름별 사용 횟수)
type ByNum = BTreeMap<(String, u32, String), (String, BTreeMap<String, usize>)>;

pub fn numerals(text: &str) -> Report {
    let mut r = Report::new("부재번호 체크");
    let mut by_num = ByNum::new();
    let mut by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for c in NUMERAL.captures_iter(text) {
        let (name, num) = (squash(&c[1]), c[2].to_string());
        *by_num.entry(natural(&num)).or_insert_with(|| (num.clone(), BTreeMap::new())).1.entry(name.clone()).or_default() += 1;
        by_name.entry(name).or_default().insert(num);
    }
    for (num, names) in by_num.values() {
        let list = names.iter().map(|(n, k)| format!("{n}({k}회)")).collect::<Vec<_>>().join(", ");
        r.info.push(format!("{num}: {list}"));
        if names.len() > 1 {
            r.problems.push(format!("부호 {num}이 여러 이름으로 쓰임: {list}"));
        }
    }
    for (name, nums) in by_name {
        if nums.len() > 1 {
            let mut nums: Vec<_> = nums.into_iter().collect();
            nums.sort_by_key(|n| natural(n));
            r.problems.push(format!("'{name}'에 부호가 여러 개: {}", nums.join(", ")));
        }
    }
    r
}

// ───────────── 반복 단어 ─────────────

pub fn repeats(text: &str) -> Report {
    let mut r = Report::new("반복 단어");
    for (i, line) in text.lines().enumerate() {
        let t: Vec<&str> = line.split_whitespace().collect();
        for n in 1..=3 {
            for k in 0..t.len().saturating_sub(2 * n - 1) {
                if t[k..k + n] == t[k + n..k + 2 * n] && t[k].chars().any(char::is_alphabetic) {
                    r.problems.push(format!("{}줄: \"{}\"", i + 1, t[k..k + 2 * n].join(" ")));
                }
            }
        }
    }
    r
}

// ───────────── 괄호 뒤 조사 ─────────────

static PAREN_JOSA: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\p{Hangul}+)\s?\([^()\n]{1,30}\)(\p{Hangul}+)").unwrap());

/// 괄호 앞 낱말의 마지막 글자에 맞는 조사. 맞으면 None.
fn fix_josa(last: char, josa: &str) -> Option<String> {
    let code = (last as u32).checked_sub(0xAC00).filter(|&c| c < 11172)?;
    let jong = code % 28; // 0 = 받침 없음, 8 = ㄹ
    let has = jong != 0;
    let pick = |a: &str, b: &str| if has { a.to_string() } else { b.to_string() };
    let want = match josa {
        "은" | "는" => pick("은", "는"),
        "이" | "가" => pick("이", "가"),
        "을" | "를" => pick("을", "를"),
        _ if (josa.starts_with('과') || josa.starts_with('와')) && josa.chars().count() <= 2 => {
            pick("과", "와") + &josa[3..]
        }
        _ if josa.starts_with("으로") && (!has || jong == 8) => format!("로{}", &josa[6..]),
        _ if josa.starts_with('로') && has && jong != 8 => format!("으{josa}"),
        _ => return None,
    };
    (want != josa).then_some(want)
}

pub fn josa(text: &str) -> Report {
    let mut r = Report::new("괄호 뒤 조사");
    for (i, line) in text.lines().enumerate() {
        for c in PAREN_JOSA.captures_iter(line) {
            let last = c[1].chars().next_back().unwrap();
            if let Some(want) = fix_josa(last, &c[2]) {
                let m = &c[0];
                let fixed = format!("{}{want}", &m[..m.len() - c[2].len()]);
                r.problems.push(format!("{}줄: {m} → {fixed}", i + 1));
            }
        }
    }
    r
}

// ───────────── 도면 ─────────────

static FIG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"도\s*(\d+)(?:\(([A-Za-z])\)|([A-Za-z]))?").unwrap());
static FIG_NEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(,|및|과|와|또는|내지|~)\s*(도\s*)?(\d+)(?:\(([A-Za-z])\)|([A-Za-z]))?").unwrap()
});

fn fig_of(c: &regex::Captures, n: usize) -> Fig {
    let suffix = c.get(n + 1).or(c.get(n + 2)).map_or(String::new(), |m| m.as_str().to_lowercase());
    (c[n].parse().unwrap_or(0), suffix)
}

fn expand(a: &Fig, b: &Fig) -> Vec<Fig> {
    let (sa, sb) = (a.1.chars().next(), b.1.chars().next());
    match (sa, sb) {
        (None, None) if a.0 < b.0 && b.0 - a.0 <= 100 => (a.0..=b.0).map(|n| (n, String::new())).collect(),
        (Some(x), Some(y)) if a.0 == b.0 && x < y => (x..=y).map(|s| (a.0, s.to_string())).collect(),
        _ => vec![a.clone(), b.clone()],
    }
}

/// "도 1", "도 2a 및 도 2b", "도 3 내지 도 5", "도 6 및 7" → 도면 번호 집합
fn figures_in(text: &str) -> BTreeSet<Fig> {
    let mut out = BTreeSet::new();
    for c in FIG.captures_iter(text) {
        let m = c.get(0).unwrap();
        if text[..m.start()].chars().next_back().is_some_and(|ch| ch.is_alphanumeric()) {
            continue; // "온도 100", "정도 1"
        }
        let mut cur = fig_of(&c, 1);
        out.insert(cur.clone());
        let mut end = m.end();
        while let Some(n) = FIG_NEXT.captures(&text[end..]) {
            let next_end = end + n.get(0).unwrap().end();
            // "도"가 없는 맨 숫자는 뒤가 조사 · 구두점일 때만 ("및 2개의"는 제외)
            let tail = text[next_end..].chars().next();
            if n.get(2).is_none() && !tail.is_none_or(|ch| !ch.is_alphanumeric() || "은는을를의에과와".contains(ch)) {
                break;
            }
            let next = fig_of(&n, 3);
            if matches!(&n[1], "내지" | "~") {
                out.extend(expand(&cur, &next));
            }
            out.insert(next.clone());
            cur = next;
            end = next_end;
        }
    }
    out
}

fn show(set: &BTreeSet<Fig>) -> String {
    if set.is_empty() {
        return "(없음)".into();
    }
    set.iter().map(|(n, s)| format!("{n}{s}")).collect::<Vec<_>>().join(", ")
}

fn diff(r: &mut Report, a_name: &str, a: &BTreeSet<Fig>, b_name: &str, b: &BTreeSet<Fig>) {
    let only_a: BTreeSet<Fig> = a.difference(b).cloned().collect();
    let only_b: BTreeSet<Fig> = b.difference(a).cloned().collect();
    if !only_a.is_empty() {
        r.problems.push(format!("{a_name}에만 있음: 도 {}", show(&only_a)));
    }
    if !only_b.is_empty() {
        r.problems.push(format!("{b_name}에만 있음: 도 {}", show(&only_b)));
    }
}

pub fn figures(spec: &Spec) -> Report {
    let mut r = Report::new("도면 체크");
    let brief = figures_in(&spec.brief);
    let body = figures_in(&spec.body);
    r.info.push(format!("도면의 간단한 설명: {}", show(&brief)));
    r.info.push(format!("발명의 설명(본문): {}", show(&body)));
    if spec.brief.trim().is_empty() {
        r.info.push("【도면의 간단한 설명】 섹션이 없어 비교 생략".into());
        return r;
    }
    diff(&mut r, "도면의 간단한 설명", &brief, "본문", &body);
    match &spec.drawings {
        Some(d) => {
            let drawn: BTreeSet<Fig> = d.iter().cloned().collect();
            r.info.push(format!("첨부 도면: {}", show(&drawn)));
            diff(&mut r, "도면의 간단한 설명", &brief, "첨부 도면", &drawn);
        }
        None => r.info.push("【도 N】 첨부 도면이 없어 도면 대조 생략".into()),
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::parse;

    const SAMPLE: &str = include_str!("../examples/sample.txt");

    fn problems(r: Report) -> String {
        r.problems.join("\n")
    }

    #[test]
    fn sample_spec() {
        let spec = parse(SAMPLE);

        let p = problems(sanggi(&spec));
        assert!(p.contains("청구항 1: 상기 절연층"), "{p}");
        assert!(!p.contains("상기 기판"), "{p}");
        assert!(!p.contains("청구항 2"), "인용항에 있는 '장치'는 통과해야 함: {p}");
        assert!(p.contains("청구항 3: 상기 제2전극"), "{p}");

        let p = problems(numerals(SAMPLE));
        assert!(p.contains("'기판'에 부호가 여러 개: 10, 11"), "{p}");

        let p = problems(repeats(SAMPLE));
        assert!(p.contains("\"투명 투명\""), "{p}");

        let p = problems(josa(SAMPLE));
        assert!(p.contains("전극(20)는 → 전극(20)은"), "{p}");
        assert!(p.contains("커버(30)은 → 커버(30)는"), "{p}");
        assert!(p.contains("기판(10)로 → 기판(10)으로"), "{p}");
        assert!(!p.contains("전극(20)을"), "{p}");

        let r = figures(&spec);
        let p = r.problems.join("\n");
        assert!(p.contains("본문에만 있음: 도 3"), "{p}");
        assert!(p.contains("첨부 도면에만 있음: 도 4b"), "{p}");

        let p = problems(citations(&spec));
        assert!(p.contains("청구항 4: 2 이상의 항을 택일적"), "{p}");
        assert!(p.contains("청구항 5: 다중인용항이 다중인용항(청구항 4)"), "{p}");
        assert!(p.contains("청구항 5: 자기 자신이나 뒤의 청구항 6"), "{p}");
    }

    #[test]
    fn figure_ranges() {
        let f = figures_in("도 1 내지 도 3, 도 4a 내지 4c 및 도 5(a)를 참조하면 온도 100에서 도 6 및 2개의");
        assert_eq!(show(&f), "1, 2, 3, 4a, 4b, 4c, 5a, 6");
    }

    #[test]
    fn josa_rules() {
        assert_eq!(fix_josa('극', "는"), Some("은".into()));
        assert_eq!(fix_josa('버', "와의"), None);
        assert_eq!(fix_josa('판', "와의"), Some("과의".into()));
        assert_eq!(fix_josa('일', "으로"), Some("로".into()));
        assert_eq!(fix_josa('일', "로부터"), None);
        assert_eq!(fix_josa('극', "이며"), None);
    }
}
