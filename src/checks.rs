//! 검사 항목. 각 함수는 참고 정보(info)와 지적 사항(problems)을 돌려준다.

use std::fmt;

use crate::spec::{Fig, Spec, squash};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

pub struct Report {
    /// JSON용 영문 식별자 (바뀌지 않음)
    pub id: &'static str,
    pub title: &'static str,
    pub info: Vec<String>,
    pub problems: Vec<Finding>,
}

/// 지적 하나. 사람이 읽을 문장과, 에이전트가 쓸 위치 · 고칠 말을 따로 둔다.
#[derive(Default)]
pub struct Finding {
    pub msg: String,
    /// 청구항 번호. 0 = 청구항 머리말이 없어 붙여 넣은 글 전체를 본 경우
    pub claim: Option<u32>,
    /// 추출한 텍스트의 줄 번호 (DOCX · HWP · HWPX는 문단 번호)
    pub line: Option<usize>,
    /// 그대로 바꾸면 되는 경우: (찾을 말, 바꿀 말)
    pub fix: Option<(String, String)>,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match (self.claim, self.line) {
            (Some(0), _) => write!(f, "본문: ")?,
            (Some(c), _) => write!(f, "청구항 {c}: ")?,
            (None, Some(l)) => write!(f, "{l}줄: ")?,
            _ => {}
        }
        f.write_str(&self.msg)
    }
}

impl Report {
    fn new(id: &'static str, title: &'static str) -> Self {
        Report { id, title, info: vec![], problems: vec![] }
    }
    fn add(&mut self, msg: String) {
        self.problems.push(Finding { msg, ..Default::default() });
    }
    fn claim(&mut self, claim: u32, msg: String) {
        self.problems.push(Finding { msg, claim: Some(claim), ..Default::default() });
    }
    fn fix(&mut self, line: usize, msg: String, find: &str, replace: String) {
        self.problems.push(Finding { msg, line: Some(line), fix: Some((find.into(), replace)), ..Default::default() });
    }
}

// ───────────── 인용항 ─────────────

pub fn citations(spec: &Spec) -> Report {
    let mut r = Report::new("citations", "인용항 체크");
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
            r.add(format!("청구항 번호가 {prev} 다음에 {}로 이어짐 (누락 · 중복 확인)", c.num));
        }
        prev = c.num;
        for &n in &c.refs {
            if n >= c.num {
                r.claim(c.num, format!("자기 자신이나 뒤의 청구항 {n}을 인용"));
            } else if find(n).is_none() {
                r.claim(c.num, format!("없는 청구항 {n}을 인용"));
            }
        }
        if c.refs.len() < 2 {
            continue;
        }
        // 특허법 시행령 제5조 제5항: 2 이상 인용 시 택일적으로 기재
        if !preamble(&c.text).contains("또는") && !preamble(&c.text).contains("어느") {
            r.claim(c.num, "2 이상의 항을 택일적(또는 / 중 어느 한 항)으로 인용하지 않음".to_string());
        }
        // 같은 조 제6항: 다중인용항은 다른 다중인용항을 인용할 수 없음
        for &n in &c.refs {
            if find(n).is_some_and(|p| p.refs.len() > 1) {
                r.claim(c.num, format!("다중인용항이 다중인용항(청구항 {n})을 인용"));
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

/// 문장 안의 "상기 X"마다 (위치, X 후보들). "상기한", "상기와"는 건너뜀.
fn sanggi_terms(text: &str) -> Vec<(usize, Vec<String>)> {
    let mut out = vec![];
    for (p, _) in text.match_indices("상기") {
        let rest = &text[p + "상기".len()..];
        let before = text[..p].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric()) || !rest.chars().next().is_some_and(char::is_whitespace) {
            continue;
        }
        let cands = term_candidates(rest);
        if !cands.is_empty() {
            out.push((p, cands));
        }
    }
    out
}

/// 화면에 보일 어간: 조사를 뗀 것이 있으면 그것
fn stem(cands: &[String]) -> String {
    cands.get(1).unwrap_or(&cands[0]).clone()
}

pub fn sanggi(spec: &Spec) -> Report {
    let mut r = Report::new("antecedent", "상기 체크");
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
        for (p, cands) in sanggi_terms(text) {
            let stem = stem(&cands);
            checked += 1;
            let earlier = squash(&text[..p]);
            if !cands.iter().any(|t| earlier.contains(t.as_str()) || cited.contains(t.as_str())) && !missing.contains(&stem) {
                missing.push(stem);
            }
        }
        for m in missing {
            r.claim(num, format!("상기 {m} — 앞에서(인용항 포함) 언급 안 됨"));
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
    Regex::new(r"([\p{Hangul}A-Za-z][\p{Hangul}A-Za-z0-9]*)\s?\(([A-Za-z]?\d+[A-Za-z0-9'-]*)\)").unwrap()
});

/// "S110" → ("S", 110, ""), "10a" → ("", 10, "a"): 사람이 읽는 순서로 정렬
fn natural(s: &str) -> (String, u32, String) {
    let i = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    let j = s[i..].find(|c: char| !c.is_ascii_digit()).map_or(s.len(), |k| i + k);
    (s[..i].to_string(), s[i..j].parse().unwrap_or(0), s[j..].to_string())
}

/// 정렬 키 → (원래 부호, 이름별 사용 횟수)
type ByNum = BTreeMap<(String, u32, String), (String, BTreeMap<String, usize>)>;

/// 이름에 붙이지 않는 앞 낱말
const NAME_STOP: &[&str] = &["상기", "및", "또는", "각", "각각", "그", "이", "해당", "본", "즉", "적어도", "하나", "모든", "다른", "소정", "또한", "더", "약", "대략", "거의", "주로", "또", "중"];
/// 이렇게 끝나는 앞 낱말은 조사 · 어미로 보고 멈춘다. 명사 끝에 흔한 '도 · 이 · 가 · 서'(온도, 센서)는 뺐다.
const NAME_BREAK: &[&str] = &[
    "에서", "의", "은", "는", "을", "를", "에", "와", "과", "로", "한", "된", "인", "될", "할", "던", "며", "고", "면",
    "게", "지", "여", "해", "라", "어", "써", "다", "히", "께",
];
/// 모든 단계가 "~하는 단계(S100)"라서 이름이 겹쳐도 정상인 것
const STEP_HEADS: &[&str] = &["단계", "과정", "동작"];

/// 괄호 바로 앞 낱말에서 앞으로 3낱말까지, 명사처럼 보이는 동안 이름에 붙인다.
/// "배치되는 제1 압력 센서(10)" → "제1 압력 센서", "'측정 구간(5)" → "측정 구간"
fn element_name(before: &str, head: &str) -> String {
    let mut words = vec![head];
    if before.ends_with(char::is_whitespace) {
        for w in before.split_whitespace().rev().take(3) {
            let bare = w.trim_start_matches(['\'', '"', '‘', '“']);
            if bare.is_empty()
                || !bare.chars().all(char::is_alphanumeric)
                || NAME_STOP.contains(&bare)
                || NAME_BREAK.iter().any(|e| bare.ends_with(e))
            {
                break;
            }
            words.push(bare);
            if bare.len() < w.len() || ORDINAL.is_match(bare) {
                break; // 따옴표가 열린 곳, "제1"이 이름의 시작
            }
        }
    }
    words.reverse();
    words.join(" ")
}

/// 이름의 마지막 낱말 ("제1 압력 센서" → "센서")
fn head(name: &str) -> &str {
    name.rsplit(' ').next().unwrap_or(name)
}

/// "이름(부호)" 쌍. "수행할 수 있다(S100)"처럼 문장 끝에 붙은 단계 부호는 이름 없이 건너뜀
fn numeral_pairs(text: &str) -> impl Iterator<Item = (String, String)> + '_ {
    NUMERAL
        .captures_iter(text)
        .filter(|c| !c[1].ends_with('다'))
        .map(|c| (element_name(&text[..c.get(0).unwrap().start()], &c[1]), c[2].to_string()))
}

fn numeral_map(text: &str) -> ByNum {
    let mut by_num = ByNum::new();
    for (name, num) in numeral_pairs(text) {
        *by_num.entry(natural(&num)).or_insert_with(|| (num, BTreeMap::new())).1.entry(name).or_default() += 1;
    }
    by_num
}

fn name_list(names: &BTreeMap<String, usize>) -> String {
    names.iter().map(|(n, k)| format!("{n}({k}회)")).collect::<Vec<_>>().join(", ")
}

/// "전극"과 "상부 전극"처럼 한쪽이 다른 쪽을 품으면 같은 구성으로 본다 (띄어쓰기 무시)
fn same_name(a: &str, b: &str) -> bool {
    let (a, b) = (squash(a), squash(b));
    a.contains(&b) || b.contains(&a)
}

pub fn numerals(text: &str) -> Report {
    let mut r = Report::new("numerals", "부재번호 체크");
    let by_num = numeral_map(text);
    // 띄어쓰기 없앤 이름 → (보여줄 이름, 부호들)
    let mut by_name: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    for (name, num) in numeral_pairs(text) {
        by_name.entry(squash(&name)).or_insert_with(|| (name, BTreeSet::new())).1.insert(num);
    }
    for (num, names) in by_num.values() {
        let list = name_list(names);
        r.info.push(format!("{num}: {list}"));
        // "제1 압력 센서"와 "압력 센서"는 줄여 쓴 것으로 보고, 마지막 낱말이나 서수(제1 · 제2)가 다를 때만
        let heads: BTreeSet<_> = names.keys().map(|n| head(n)).collect();
        let ordinals: BTreeSet<_> = names.keys().filter_map(|n| n.split(' ').find(|w| ORDINAL.is_match(w))).collect();
        if heads.len() > 1 || ordinals.len() > 1 {
            r.add(format!("부호 {num}이 여러 이름으로 쓰임: {list}"));
        }
    }
    for (name, nums) in by_name.into_values() {
        if nums.len() > 1 && !STEP_HEADS.iter().any(|h| name.ends_with(h)) {
            let mut nums: Vec<_> = nums.into_iter().collect();
            nums.sort_by_key(|n| natural(n));
            r.add(format!("'{name}'에 부호가 여러 개: {}", nums.join(", ")));
        }
    }
    r
}

// ───────────── 불명확한 표현 ─────────────

static VAGUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^\w])(약\s?\d|대략|거의|바람직|필요에\s?따라|소망에\s?따라|필요시|필요한\s?경우|예를\s?들어|예컨대|특히|주로|등(?:의|을|이|과|으로|에|은|도)?(?:[^\p{Hangul}]|$))").unwrap()
});

pub fn vague(spec: &Spec) -> Report {
    let mut r = Report::new("vague", "불명확한 표현");
    for c in &spec.claims {
        let mut seen = BTreeSet::new();
        for m in VAGUE.captures_iter(&c.text) {
            let word = m[1].trim_end_matches(|ch: char| !ch.is_alphanumeric()).to_string();
            if seen.insert(word.clone()) {
                r.claim(c.num, format!("'{word}' — 권리범위가 불명확해질 수 있음"));
            }
        }
    }
    r
}

// ───────────── 부호의 설명 대조 ─────────────

static SIGN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([A-Za-z]?\d+[A-Za-z0-9']*)\s*[:：]\s*").unwrap());

/// "10: 기판 20: 제1 전극, 30: 커버" → [(10, 기판), (20, 제1전극), (30, 커버)]
fn sign_list(signs: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    for line in signs.lines() {
        let ms: Vec<_> = SIGN.captures_iter(line).collect();
        for (k, c) in ms.iter().enumerate() {
            let end = ms.get(k + 1).map_or(line.len(), |n| n.get(0).unwrap().start());
            let name = line[c.get(0).unwrap().end()..end].trim_matches(|ch: char| ch.is_whitespace() || ",;".contains(ch));
            out.push((c[1].to_string(), squash(name)));
        }
    }
    out
}

pub fn signs(spec: &Spec, text: &str) -> Report {
    let mut r = Report::new("sign_list", "부호의 설명 대조");
    if spec.signs.trim().is_empty() {
        r.info.push("【부호의 설명】 섹션이 없어 건너뜀".into());
        return r;
    }
    let listed = sign_list(&spec.signs);
    let used = numeral_map(text);
    r.info.push(format!("부호의 설명 {}개", listed.len()));
    let mut keys = BTreeSet::new();
    for (num, name) in &listed {
        if !keys.insert(natural(num)) {
            r.add(format!("부호 {num}: 부호의 설명에 두 번 나옴"));
        }
        match used.get(&natural(num)) {
            None => r.add(format!("부호 {num}({name}): 본문 · 청구항에서 안 쓰임")),
            Some((_, names)) if !names.keys().any(|n| same_name(n, name)) => {
                r.add(format!("부호 {num}: 부호의 설명은 '{name}', 본문은 {}", name_list(names)));
            }
            _ => {}
        }
    }
    for (key, (num, names)) in &used {
        if !keys.contains(key) {
            r.add(format!("부호 {num}: 부호의 설명에 없음 — 본문은 {}", name_list(names)));
        }
    }
    r
}

// ───────────── 청구항 형식 ─────────────

pub fn claim_form(spec: &Spec) -> Report {
    let mut r = Report::new("claim_form", "청구항 형식");
    let body = numeral_map(&spec.body);
    for c in &spec.claims {
        let t = c.text.trim();
        let ch: Vec<char> = t.chars().collect();
        // 소수점(0.5)은 마침표로 세지 않는다
        let dots = (0..ch.len())
            .filter(|&i| ch[i] == '.' && !(i > 0 && ch[i - 1].is_ascii_digit() && ch.get(i + 1).is_some_and(char::is_ascii_digit)))
            .count();
        if !t.ends_with('.') {
            r.claim(c.num, "마침표로 끝나지 않음".to_string());
        }
        if dots > 1 {
            r.claim(c.num, format!("마침표가 {dots}개 — 청구항은 한 문장이어야 함"));
        }
        let mut seen = BTreeSet::new();
        for (name, num) in numeral_pairs(t) {
            if !seen.insert((name.clone(), num.clone())) {
                continue;
            }
            match body.get(&natural(&num)) {
                None => r.claim(c.num, format!("{name}({num}) — 발명의 설명에 없는 부호")),
                Some((_, names)) if !names.keys().any(|n| same_name(n, &name)) => {
                    r.claim(c.num, format!("{name}({num}) — 발명의 설명에서 {num}은 {}", name_list(names)))
                }
                _ => {}
            }
        }
    }
    r
}

// ───────────── 청구항 용어 뒷받침 ─────────────

/// 청구항의 구성(상기 X, 이름(부호))이 발명의 설명에 나오는지. 특허법 제42조 제4항 제1호
pub fn support(spec: &Spec) -> Report {
    let mut r = Report::new("support", "청구항 용어 뒷받침");
    if spec.body.trim().is_empty() {
        r.info.push("발명의 설명이 없어 건너뜀".into());
        return r;
    }
    let body = squash(&spec.body);
    let mut seen = BTreeSet::new();
    for c in &spec.claims {
        let terms = sanggi_terms(&c.text).into_iter().map(|(_, t)| t).chain(numeral_pairs(&c.text).map(|(n, _)| vec![n.clone(), head(&n).to_string()]));
        for cands in terms {
            let s = stem(&cands);
            if seen.insert(s.clone()) && !cands.iter().any(|t| body.contains(&squash(t))) {
                r.claim(c.num, format!("'{s}' — 발명의 설명에 없음"));
            }
        }
    }
    r.info.push(format!("구성 용어 {}개 확인", seen.len()));
    r
}

// ───────────── 반복 단어 ─────────────

pub fn repeats(text: &str) -> Report {
    let mut r = Report::new("repeats", "반복 단어");
    for (i, line) in text.lines().enumerate() {
        let t: Vec<&str> = line.split_whitespace().collect();
        for n in 1..=3 {
            for k in 0..t.len().saturating_sub(2 * n - 1) {
                if t[k..k + n] == t[k + n..k + 2 * n] && t[k].chars().any(char::is_alphabetic) {
                    let (twice, once) = (t[k..k + 2 * n].join(" "), t[k..k + n].join(" "));
                    r.fix(i + 1, format!("\"{twice}\""), &twice, once);
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
    let mut r = Report::new("particles", "괄호 뒤 조사");
    for (i, line) in text.lines().enumerate() {
        for c in PAREN_JOSA.captures_iter(line) {
            let last = c[1].chars().next_back().unwrap();
            if let Some(want) = fix_josa(last, &c[2]) {
                let m = &c[0];
                let fixed = format!("{}{want}", &m[..m.len() - c[2].len()]);
                r.fix(i + 1, format!("{m} → {fixed}"), m, fixed);
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
        r.add(format!("{a_name}에만 있음: 도 {}", show(&only_a)));
    }
    if !only_b.is_empty() {
        r.add(format!("{b_name}에만 있음: 도 {}", show(&only_b)));
    }
}

pub fn figures(spec: &Spec) -> Report {
    let mut r = Report::new("figures", "도면 체크");
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
        r.problems.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("\n")
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
        let p = problems(r);
        assert!(p.contains("본문에만 있음: 도 3"), "{p}");
        assert!(p.contains("첨부 도면에만 있음: 도 4b"), "{p}");

        let p = problems(citations(&spec));
        assert!(p.contains("청구항 4: 2 이상의 항을 택일적"), "{p}");
        assert!(p.contains("청구항 5: 다중인용항이 다중인용항(청구항 4)"), "{p}");
        assert!(p.contains("청구항 5: 자기 자신이나 뒤의 청구항 6"), "{p}");

        let p = problems(vague(&spec));
        assert!(p.contains("'약 5'") && p.contains("'특히'"), "{p}");
        assert!(!vague(&parse("【청구항 1】\n균등한 등록 등이방성 장치.")).problems.iter().any(|x| x.msg.contains('등')));

        let p = problems(signs(&spec, SAMPLE));
        assert!(p.contains("부호 50(스페이서): 본문 · 청구항에서 안 쓰임"), "{p}");
        assert!(p.contains("부호 11: 부호의 설명에 없음"), "{p}");
        assert!(!p.contains("부호 10"), "{p}");

        let p = problems(claim_form(&spec));
        assert!(p.contains("청구항 6: 마침표가 2개"), "{p}");
        assert!(p.contains("청구항 6: 받침대(40) — 발명의 설명에 없는 부호"), "{p}");
        assert!(p.contains("청구항 7: 마침표로 끝나지 않음"), "{p}");
        assert!(!p.contains("청구항 1:"), "{p}");

        let p = problems(support(&spec));
        assert!(p.contains("청구항 1: '절연층'"), "{p}");
        assert!(!p.contains("'기판'"), "{p}");
    }

    #[test]
    fn element_names() {
        let names: Vec<_> = numeral_pairs(
            "기판 상에 배치되는 제1 광 센서(11)와, 그 중 제1 광 센서(11), '측정 구간(5)은 프레임 하우징(3)에서 상기 센서(10) 및 보조 센서(12)를 수행할 수 있다(S10). 측정하는 단계(S20)",
        )
        .map(|(n, k)| format!("{k}={n}"))
        .collect();
        assert_eq!(names, ["11=제1 광 센서", "11=제1 광 센서", "5=측정 구간", "3=프레임 하우징", "10=센서", "12=보조 센서", "S20=단계"]);

        let p = problems(numerals("제1 광 센서(11), 광 센서(11), 하는 단계(S1), 되는 단계(S2), 기판(13), 커버(13), 제1 판(14), 제2 판(14)"));
        assert_eq!(p, "부호 13이 여러 이름으로 쓰임: 기판(1회), 커버(1회)\n부호 14이 여러 이름으로 쓰임: 제1 판(1회), 제2 판(1회)");
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
