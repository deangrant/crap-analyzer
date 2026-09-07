use super::*;
use crap_core::{FunctionComplexity, Metric};
use std::path::Path;

fn parse(src: &str, metric: Metric) -> Vec<FunctionComplexity> {
    let parsed = analyze_source(Path::new("t.ts"), src, metric);
    assert!(parsed.is_ok(), "{parsed:?}");
    parsed.unwrap_or_default()
}

fn names(src: &str) -> Vec<String> {
    parse(src, Metric::Cyclomatic).into_iter().map(|f| f.name).collect()
}

fn cyclo(src: &str) -> Vec<usize> {
    parse(src, Metric::Cyclomatic).into_iter().map(|f| f.complexity).collect()
}

#[test]
fn named_function_and_method() {
    let src = r"
export function top(x: number) { return x; }
class Box {
  static make() { return new Box(); }
  get value() { return 1; }
  set value(_v: number) {}
  run() { return 2; }
}
";
    let found = names(src);
    assert!(found.contains(&"top".into()));
    assert!(found.contains(&"Box.make".into()));
    assert!(found.contains(&"Box.get value".into()));
    assert!(found.contains(&"Box.set value".into()));
    assert!(found.contains(&"Box.run".into()));
}

#[test]
fn assigned_function_and_block_arrow() {
    let src = r"
const a = function (x: number) { return x; };
const b = async (x: number) => { return x; };
let c = (x: number) => { return x; };
var d = (x: number) => x;
";
    let found = names(src);
    assert!(found.contains(&"a".into()));
    assert!(found.contains(&"b".into()));
    assert!(found.contains(&"c".into()));
    assert!(!found.contains(&"d".into()));
}

#[test]
fn declare_without_body_is_skipped() {
    let src = "declare function ghost(): void;\nfunction real() { return 1; }\n";
    assert_eq!(names(src), vec!["real".to_owned()]);
}

#[test]
fn nested_function_is_separate_row() {
    let src = r"
function outer() {
  function inner() { return 1; }
  return inner();
}
";
    let found = names(src);
    assert_eq!(found, vec!["outer".to_owned(), "inner".to_owned()]);
    assert_eq!(cyclo(src), vec![1, 1]);
}

#[test]
fn analyze_tsx_class_component_method() {
    let src = r#"
export class App {
  render() {
    return <div>{true && "x"}</div>;
  }
}
"#;
    let fns = {
        let parsed = analyze_source(Path::new("t.tsx"), src, Metric::Cyclomatic);
        assert!(parsed.is_ok(), "{parsed:?}");
        parsed.unwrap_or_default()
    };
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "App.render");
    assert!(fns[0].complexity >= 2);
}

#[test]
fn async_generator_and_typed_params() {
    let src = r"
export async function* gen<T>(x: T): AsyncGenerator<T> { yield x; }
function withTypes(x: number): number { return x; }
";
    let found = names(src);
    assert!(found.contains(&"gen".into()));
    assert!(found.contains(&"withTypes".into()));
}

#[test]
fn class_with_heritage_and_decorators() {
    let src = r"
@sealed
export class Child extends Base implements IFace {
  @bind
  public async run(): Promise<void> { await 1; }
  private [key]() { return 1; }
}
";
    let found = names(src);
    assert!(found.iter().any(|n| n.starts_with("Child.")));
}

#[test]
fn export_default_function() {
    let src = "export default function main() { return 1; }\n";
    assert_eq!(names(src), vec!["main".to_owned()]);
}

#[test]
fn assigned_named_function_expr() {
    let src = "const a = function named(x: number) { return x; };\n";
    assert_eq!(names(src), vec!["a".to_owned()]);
}

#[test]
fn overload_signatures_skipped() {
    let src = r"
function over(x: string): string;
function over(x: number): number;
function over(x: string | number) { return x; }
";
    assert_eq!(names(src), vec!["over".to_owned()]);
}

#[test]
fn class_without_name_is_skipped_gracefully() {
    let src = "class { run() { return 1; } }\n";
    let found = names(src);
    assert!(found.is_empty() || found.iter().any(|n| n.contains("run")));
}

#[test]
fn modifiers_on_methods() {
    let src = r"
class C {
  readonly x = 1;
  protected static async foo() { return 1; }
  abstract bar(): void;
  override baz() { return 2; }
}
";
    let found = names(src);
    assert!(found.contains(&"C.foo".into()));
    assert!(found.contains(&"C.baz".into()));
}

#[test]
fn assigned_binding_edges() {
    assert!(names("foo = () => { return 1; }\n").is_empty());
    assert!(names("const x: number = 1;\n").is_empty());
    assert!(names("const x;\n").is_empty());
    assert!(names("const = () => { return 1; }\n").is_empty());
    let found = names("const a: Fn = function () { return 1; };\n");
    assert!(found.contains(&"a".into()));
    let typed = names("const b: Array<[number]> = function () { return []; };\n");
    assert!(typed.contains(&"b".into()));
}

#[test]
fn typeish_bracket_pairs() {
    assert!(super::skip_typeish(b"(n: number) => void", 0) > 1);
    assert!(super::skip_typeish(b"[number]", 0) > 1);
    assert!(super::skip_typeish(b"<T>", 0) > 1);
}

#[test]
fn arrow_and_params_edges() {
    assert!(names("const a = x { return 1; };\n").is_empty());
    assert!(names("const a = x => { return x; };\n").contains(&"a".to_owned()));
    assert!(names("function () { return 1; }\n").is_empty());
    assert!(names("function foo { return 1; }\n").is_empty());
    let found =
        names("function typed(x: Array<{ a: number }>): { b: string } { return { b: '' }; }\n");
    assert!(found.contains(&"typed".into()));
}

#[test]
fn class_unclosed_body_and_decorator_call() {
    let bad = analyze_source(Path::new("t.ts"), "class Foo {\n", Metric::Cyclomatic);
    assert!(bad.is_err(), "{bad:?}");
    assert!(names("class Foo\n").is_empty());
    let found = names(
        r"
@dec(1)
class C {
  @ann()
  m() { return 1; }
}
class Empty {}
class Noise {/**/}
",
    );
    assert!(found.contains(&"C.m".into()));
}

#[test]
fn method_computed_name() {
    let src = r"
class C {
  [sym](x: number) { return x; }
  bad { return 1; }
}
";
    let found = names(src);
    assert!(found.iter().any(|n| n.starts_with("C.")));
}

#[test]
fn trailing_noise_only_source() {
    assert!(names("// only").is_empty());
}

#[test]
fn analyze_source_rejects_unclosed_comment() {
    let src = "/* open\nfunction f() {}\n";
    let err = analyze_source(Path::new("t.ts"), src, Metric::Cyclomatic);
    assert!(err.is_err(), "{err:?}");
    assert!(format!("{err:?}").contains("unclosed comment"), "{err:?}");
}
