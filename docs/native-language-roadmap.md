# Zsharpend Language Evolution

This roadmap changes Zsharpend from a Rust-flavored compatibility layer into a language with a distinct syntax, runtime model, package workflow, and self-hosting trajectory. Rust remains a bootstrap implementation detail; ordinary users should not need Rust to use the language.

## Goals

- Preserve existing Z#++/.zsp/.zsx/.zsg compatibility and keep .zxx as the modern dialect.
- Establish a native-feeling syntax rather than accumulating Python, Rust, and HolyC syntax variants as the language identity.
- Add interpreted/runtime type declarations and reflection, including safe conversions and type checks.
- Ship ZAP as the official package, project, build, test, and dependency command.
- Make the interpreter and VM implementation portable and embeddable; eventually bootstrap the toolchain in Zsharpend itself.

## Proposed syntax direction

Use declaration forms such as `name: text = "value"`, inferred bindings `name := expression`, function declarations `func name(args) -> result`, and blocks with braces. Favor readable language-native keywords and expressions. Avoid making Rust syntax (`fn`, `let mut`, `trait`, `?`) mandatory; keep these as optional compatibility features where practical.

## Runtime and types

Provide these primitive types: int, float, number, text, bool, byte, char, null, and void. Add list<T>, array<T>, map<K,V>, tuple, optional<T>, result<T,E>, and user-defined struct/enum types. Runtime values can use dynamic/any where explicitly requested. Runtime reflection should expose type(value), value.is(Type), value.as(Type), and safe conversion results. Static/inferred declarations should be checked before execution when possible; dynamic values are checked at runtime.

## ZAP command line

ZAP is the project/package manager. Planned commands: zap init, zap run, zap build, zap check, zap test, zap add, zap remove, zap update, zap upgrade, zap search, zap info, zap install, zap uninstall, zap list, zap publish, zap clean. Use zap.json for project metadata and zap.lock for reproducible dependency versions. Initially support local path packages and the existing Whetstone module format; add a registry and remote version resolution after the lockfile and package integrity design are implemented.

## Independent implementation plan

1. Syntax specification and compatibility tests.
2. Runtime type objects, type reflection, conversion and dynamic-value behavior.
3. Native syntax conveniences and parser/lexer tests.
4. ZAP CLI, project templates, local dependencies and lockfile.
5. VM bytecode and optimization; keep interpretation as a first-class mode.
6. Standard library API stabilization.
7. Self-hosting milestones: write ZAP/runtime/compiler components in Zsharpend and bootstrap from the current implementation.

## Compatibility and quality gates

Each stage must retain existing fixtures and tests. Add focused tests for old dialects and new syntax, deterministic package installs, dependency cycles, malformed manifests, runtime conversion failures, and interpreter/VM parity. Avoid big-bang rewrites and keep CI passing between commits.
