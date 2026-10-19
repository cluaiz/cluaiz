//! Number spell-out from CLDR rule-based number format (RBNF) rules, the data ICU and Android use
//! to say numbers as words in each language. A language's `data/numbers/<lang>.json` carries its
//! rule sets (`spellout`) and plural rules (`plural`); this module reads them for whole numbers.
//!
//! Rule text follows the ICU RBNF syntax as written in cldr-json: `←←` / `→→` format the quotient
//! / remainder by the rule's divisor (a power of 10 at or below its base value), `←%set←`,
//! `→%set→` and `=%set=` use another rule set, `=#,##0=` writes digits, `[…]` is left out when the
//! remainder is zero, and `$(cardinal,one{…}other{…})$` picks a plural form for the quotient.

use std::collections::HashMap;

/// One rule: applies from `base` up to the next rule's base.
#[derive(Debug, Clone)]
struct Rule {
    base: u64,
    divisor: u64,
    body: String,
}

#[derive(Debug, Default)]
pub(super) struct Spellout {
    sets: HashMap<String, Vec<Rule>>,
    entry: String,
    plural: Vec<(String, String)>,
}

impl Spellout {
    /// `sets`: rule set name → `[descriptor, body]` pairs in CLDR order; `entry`: the set used for
    /// plain numbers; `plural`: cardinal category → CLDR plural condition (`other` is the default).
    pub(super) fn new(
        sets: &HashMap<String, Vec<(String, String)>>,
        entry: &str,
        plural: &[(String, String)],
    ) -> Option<Self> {
        let mut parsed = HashMap::new();
        for (name, rules) in sets {
            let mut list: Vec<Rule> = rules
                .iter()
                .filter_map(|(descriptor, body)| {
                    // Whole-number rules only ("100", "100/1000"); "-x", "x.x" and the like are not used
                    let (value, radix) = descriptor.split_once('/').unwrap_or((descriptor, "10"));
                    let base: u64 = value.trim().parse().ok()?;
                    let radix: u64 = radix.trim().parse().ok().filter(|r| *r >= 2)?;
                    let mut divisor = 1u64;
                    while divisor.checked_mul(radix).is_some_and(|d| d <= base) {
                        divisor *= radix;
                    }
                    // Leading spaces are ignored unless an apostrophe keeps them ("' e …")
                    let body = body.trim_start().trim_end().trim_end_matches(';');
                    let body = body.strip_prefix('\'').unwrap_or(body);
                    Some(Rule { base, divisor, body: body.to_owned() })
                })
                .collect();
            list.sort_by_key(|rule| rule.base);
            parsed.insert(name.clone(), list);
        }
        parsed.contains_key(entry).then(|| Self {
            sets: parsed,
            entry: entry.to_owned(),
            plural: plural.to_vec(),
        })
    }

    /// `n` in words, or None when the rules do not cover it.
    pub(super) fn words(&self, n: u64) -> Option<String> {
        let text = self.format(&self.entry, n, 0)?;
        let text = text.replace('\u{AD}', "");
        Some(text.split_whitespace().collect::<Vec<_>>().join(" "))
    }

    fn format(&self, set: &str, n: u64, depth: usize) -> Option<String> {
        if depth > 32 {
            return None;
        }
        let rules = self.sets.get(set)?;
        let index = rules.iter().rposition(|rule| rule.base <= n)?;
        let mut rule = &rules[index];
        // ICU roll-back: a multiple of the divisor whose rule base is not one uses the rule before
        if index > 0 && n % rule.divisor == 0 && rule.base % rule.divisor != 0 && rule.body.contains(['→', '>']) {
            rule = &rules[index - 1];
        }
        self.apply(set, rule, n, depth)
    }

    fn apply(&self, set: &str, rule: &Rule, n: u64, depth: usize) -> Option<String> {
        let (quotient, remainder) = (n / rule.divisor, n % rule.divisor);
        let body: Vec<char> = rule.body.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < body.len() {
            let c = body[i];
            match c {
                '[' => {
                    if remainder == 0 {
                        // Left out: skip it whole, its rules need not cover zero
                        i = body[i..].iter().position(|&x| x == ']').map_or(body.len(), |p| i + p + 1);
                        continue;
                    }
                    i += 1;
                }
                ']' => i += 1,
                '←' | '<' | '→' | '>' | '=' => {
                    let end = body[i + 1..].iter().position(|&x| x == c)? + i + 1;
                    let inner: String = body[i + 1..end].iter().collect();
                    let mut after = end + 1;
                    // "→→→" (remainder by the preceding rule) is read as "→→"
                    while after < body.len() && body[after] == c {
                        after += 1;
                    }
                    let value = match c {
                        '←' | '<' => quotient,
                        '→' | '>' => remainder,
                        _ => n,
                    };
                    let target = inner.trim();
                    let spoken = if target.starts_with('%') {
                        self.format(target, value, depth + 1)?
                    } else if target.starts_with('#') || target.starts_with('0') {
                        value.to_string()
                    } else if c == '=' {
                        // "==" would repeat this rule set for the same number
                        return None;
                    } else {
                        self.format(set, value, depth + 1)?
                    };
                    out.push_str(&spoken);
                    i = after;
                }
                '$' if body.get(i + 1) == Some(&'(') => {
                    let end = (i..body.len().saturating_sub(1)).find(|&k| body[k] == ')' && body[k + 1] == '$')?;
                    let inner: String = body[i + 2..end].iter().collect();
                    out.push_str(&self.plural_form(&inner, quotient)?);
                    i = end + 2;
                }
                _ => {
                    out.push(c);
                    i += 1;
                }
            }
        }
        Some(out)
    }

    /// `cardinal,one{tysiąc}few{tysiące}other{tysięcy}` → the form for `n`.
    fn plural_form(&self, spec: &str, n: u64) -> Option<String> {
        let (_, forms) = spec.split_once(',')?;
        let mut options: Vec<(&str, &str)> = Vec::new();
        let mut rest = forms;
        while let Some(open) = rest.find('{') {
            let close = rest[open..].find('}')? + open;
            options.push((rest[..open].trim(), &rest[open + 1..close]));
            rest = &rest[close + 1..];
        }
        let category = self
            .plural
            .iter()
            .find(|(cat, condition)| {
                options.iter().any(|(c, _)| c == cat) && !condition.trim().is_empty() && plural_matches(condition, n)
            })
            .map(|(cat, _)| cat.as_str())
            .unwrap_or("other");
        options
            .iter()
            .find(|(c, _)| *c == category)
            .or_else(|| options.iter().find(|(c, _)| *c == "other"))
            .map(|(_, form)| (*form).to_owned())
    }
}

/// A CLDR plural condition (`v = 0 and i % 10 = 2..4 and i % 100 != 12..14 or …`) for a whole
/// number: `n` and `i` are the number, the fraction operands are zero.
fn plural_matches(condition: &str, n: u64) -> bool {
    condition.split(" or ").any(|all| {
        all.split(" and ").all(|relation| {
            let (expr, negate, ranges) = if let Some((e, r)) = relation.split_once("!=") {
                (e, true, r)
            } else if let Some((e, r)) = relation.split_once('=') {
                (e, false, r)
            } else {
                return false;
            };
            let mut parts = expr.split('%');
            let value = match parts.next().map(str::trim) {
                Some("n" | "i") => n,
                Some(_) => 0, // v, w, f, t, c, e: no fraction or exponent
                None => return false,
            };
            let value = match parts.next().and_then(|m| m.trim().parse::<u64>().ok()) {
                Some(modulus) if modulus > 0 => value % modulus,
                _ => value,
            };
            let hit = ranges.split(',').any(|range| {
                let range = range.trim();
                match range.split_once("..") {
                    Some((lo, hi)) => match (lo.trim().parse::<u64>(), hi.trim().parse::<u64>()) {
                        (Ok(lo), Ok(hi)) => (lo..=hi).contains(&value),
                        _ => false,
                    },
                    None => range.parse::<u64>().is_ok_and(|x| x == value),
                }
            });
            hit != negate
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn quotient_remainder_optional_text_and_other_sets() {
        // A cut-down Spanish rule set from CLDR
        let mut sets = HashMap::new();
        sets.insert(
            "%spellout-numbering".to_string(),
            rules(&[("0", "cero;"), ("1", "uno;"), ("2", "dos;"), ("3", "tres;"), ("20", "veinte;"),
                ("21", "veinti→→;"), ("30", "treinta[ y →→];"), ("100", "cien;"), ("101", "ciento →→;"),
                ("200", "doscientos[ →→];"), ("1000", "mil[ →→];"), ("2000", "←%spellout-cardinal← mil[ →→];")]),
        );
        sets.insert("%spellout-cardinal".to_string(), rules(&[("0", "=%spellout-numbering=;")]));
        let s = Spellout::new(&sets, "%spellout-numbering", &[]).unwrap();
        assert_eq!(s.words(30).unwrap(), "treinta");
        assert_eq!(s.words(32).unwrap(), "treinta y dos");
        assert_eq!(s.words(100).unwrap(), "cien");
        assert_eq!(s.words(102).unwrap(), "ciento dos");
        assert_eq!(s.words(2023).unwrap(), "dos mil veintitres");
        assert_eq!(s.words(3000).unwrap(), "tres mil");
    }

    #[test]
    fn plural_forms_follow_the_cldr_conditions() {
        let plural = vec![
            ("one".to_string(), "i = 1 and v = 0".to_string()),
            ("few".to_string(), "v = 0 and i % 10 = 2..4 and i % 100 != 12..14".to_string()),
            ("other".to_string(), String::new()),
        ];
        let mut sets = HashMap::new();
        sets.insert(
            "%n".to_string(),
            rules(&[("0", "zero;"), ("2", "dwa;"), ("5", "pięć;"), ("1000", "tysiąc[ →→];"),
                ("2000", "←← $(cardinal,few{tysiące}other{tysięcy})$[ →→];")]),
        );
        let s = Spellout::new(&sets, "%n", &plural).unwrap();
        assert_eq!(s.words(2000).unwrap(), "dwa tysiące");
        assert_eq!(s.words(5000).unwrap(), "pięć tysięcy");
        assert!(plural_matches("i % 100 != 12..14", 22));
        assert!(!plural_matches("i % 100 != 12..14", 112));
    }
}
