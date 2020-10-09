//! TypeScript specific functions.
//!
//! Most of the functions do not check if the parser is configured for TypeScript.
//! Functions that do check will say so in the docs.

use super::expr::{identifier_reference, literal, template};
use crate::ast::Template;
use crate::{SyntaxKind::*, *};

pub const BASE_TS_RECOVERY_SET: TokenSet = token_set![
    T![void],
    T![ident],
    T![ident],
    T![await],
    T![null],
    T![break],
    T!['['],
];

pub fn ts_type(p: &mut Parser) -> Option<CompletedMarker> {
    unimplemented!();
}

pub fn ts_non_array_type(p: &mut Parser) -> Option<CompletedMarker> {
    match p.cur() {
        T![ident] | T![void] | T![yield] | T![null] | T![await] | T![break] => {
            if p.cur_src() == "asserts" && p.nth_at(1, T![this]) {
                p.bump_any();
                return ts_this_predicate(p);
            }

            let kind = match p.cur_src() {
                "void" => TS_VOID,
                "null" => TS_NULL,
                "any" => TS_ANY,
                "boolean" => TS_BOOLEAN,
                "bigint" => TS_BIGINT,
                "never" => TS_NEVER,
                "number" => TS_NUMBER,
                "object" => TS_OBJECT,
                "string" => TS_STRING,
                "symbol" => TS_SYMBOL,
                "unknown" => TS_UNKNOWN,
                "undefined" => TS_UNDEFINED,
                _ =>
                /* dummy value */
                {
                    ERROR
                }
            };

            if kind != ERROR && !p.nth_at(1, T![.]) {
                let m = p.start();
                p.bump_any();
                Some(m.complete(p, kind))
            } else {
                ts_type_ref(p, None)
            }
        }
        NUMBER | STRING | TRUE_KW | FALSE_KW | REGEX => {
            Some(literal(p).unwrap().precede(p).complete(p, TS_LITERAL))
        }
        BACKTICK => {
            let complete = template(p, None);
            // TODO: we can do this more efficiently by just looking at each event
            let parsed = p.parse_marker::<Template>(&complete);
            for elem in parsed.elements() {
                let err = p
                    .err_builder(
                        "template literals used as TypeScript types may not contain expressions",
                    )
                    .primary(elem.range(), "");

                p.error(err);
            }
            Some(complete.precede(p).complete(p, TS_TEMPLATE))
        }
        T![-] => {
            let m = p.start();
            p.bump_any();
            if p.at(NUMBER) {
                let _m = p.start();
                p.bump_any();
                _m.complete(p, LITERAL);
            } else {
                p.expect(NUMBER);
            }
            Some(m.complete(p, TS_LITERAL))
        }
        T![import] => todo!("import type"),
        T![this] => {
            if p.nth_src(1) == "is" {
                ts_this_predicate(p)
            } else {
                let m = p.start();
                p.bump_any();
                Some(m.complete(p, TS_THIS))
            }
        }
        T![typeof] => todo!("type query"),
        T!['{'] => todo!("mapped type or type_lit"),
        T!['['] => todo!("tuples"),
        T!['('] => {
            let m = p.start();
            p.bump_any();
            ts_type(p);
            p.expect(T![')']);
            Some(m.complete(p, TS_PAREN))
        }
        _ => {
            let err = p
                .err_builder("expected a type")
                .primary(p.cur_tok().range, "");

            p.err_recover(
                err,
                BASE_TS_RECOVERY_SET.union(token_set![
                    T![typeof],
                    T!['{'],
                    T!['['],
                    T!['('],
                    T![this],
                    T![import],
                    T![-],
                    NUMBER,
                    STRING,
                    TRUE_KW,
                    FALSE_KW,
                    REGEX,
                    BACKTICK
                ]),
                false,
            );
            None
        }
    }
}

/// A `this` type predicate such as `asserts this is foo` or `this is foo`, or `asserts this`
pub fn ts_this_predicate(p: &mut Parser) -> Option<CompletedMarker> {
    let m = p.start();
    let mut advanced = false;

    if p.cur_src() == "asserts" {
        p.bump_any();
        advanced = true;
    }

    if p.expect(T![this]) {
        advanced = true;
    }

    if p.cur_src() == "is" {
        p.bump_any();
        ts_type(p);
        advanced = true;
    }

    if !advanced {
        m.abandon(p);
        None
    } else {
        Some(m.complete(p, TS_PREDICATE))
    }
}

fn maybe_eat_incorrect_modifier(p: &mut Parser) -> Option<CompletedMarker> {
    if matches!(p.cur_src(), "public" | "private" | "protected" | "readonly") {
        let m = p.start();
        p.bump_any();
        Some(m.complete(p, ERROR))
    } else {
        None
    }
}

pub fn ts_type_ref(
    p: &mut Parser,
    recovery_set: impl Into<Option<TokenSet>> + Clone,
) -> Option<CompletedMarker> {
    let m = p.start();
    if let Some(err_m) = maybe_eat_incorrect_modifier(p) {
        let err = p
            .err_builder("a parameter property is only allowed in a constructor implementation")
            .primary(err_m.range(p), "");

        p.error(err);
    }

    ts_entity_name(p, recovery_set, true)?;
    if !p.has_linebreak_before_n(0) && p.at(T![<]) {
        todo!("type args");
    }

    Some(m.complete(p, TS_TYPE_REF))
}

pub fn ts_entity_name(
    p: &mut Parser,
    recovery_set: impl Into<Option<TokenSet>> + Clone,
    allow_reserved: bool,
) -> Option<CompletedMarker> {
    let init = ts_type_name(p, recovery_set.clone(), false)?;
    // TODO: maybe we should recover if no init at this point?

    let mut lhs = init;
    let set = recovery_set
        .into()
        .unwrap_or(BASE_TS_RECOVERY_SET)
        .union(token_set![T![.]]);

    while p.at(T![.]) {
        let m = lhs.precede(p);
        // TODO: we should maybe move recovery out of ts_type_name since we dont need recovery here
        ts_type_name(p, set, allow_reserved);
        lhs = m.complete(p, TS_QUALIFIED_PATH);
    }
    Some(lhs)
}

pub fn ts_type_name(
    p: &mut Parser,
    recovery_set: impl Into<Option<TokenSet>>,
    allow_reserved: bool,
) -> Option<CompletedMarker> {
    if p.at(T![ident]) || (p.cur().is_keyword() && allow_reserved) {
        let m = p.start();
        p.bump_remap(T![ident]);
        return Some(m.complete(p, TS_TYPE_NAME));
    }

    let set = recovery_set.into().unwrap_or(BASE_TS_RECOVERY_SET);
    let err = p
        .err_builder(&format!(
            "expected a TypeScript type name, but instead found `{}`",
            p.cur_src()
        ))
        .primary(p.cur_tok().range, "");

    p.err_recover(err, set, false);
    None
}
