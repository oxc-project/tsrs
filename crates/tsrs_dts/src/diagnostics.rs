use tsrs_ast::{self as ast, Node};
use tsrs_compiler::{Context, Program, ProgramLike};
use tsrs_core::{P, arena::Region};

use crate::{Result, project};

type Snapshot = P<tsrs_incremental::Program>;

// These pointers are used only inside one build. Both compiler owners must
// outlive this region, including on errors. No snapshot enters BuildSession.
pub(crate) struct Diagnostics {
    source: Option<Snapshot>,
    region: Region,
}

impl Diagnostics {
    pub fn new(source: &'static Program) -> Self {
        let region = Region::new_scratch(1 << 20);
        let scope = region.enter();
        let source = Some(snapshot(source, None));
        drop(scope);
        Self { source, region }
    }

    pub fn check(
        &self,
        source: &'static Program,
        fresh: impl FnOnce() -> Result<project::ProgramOwner>,
    ) -> Result<()> {
        let scope = self.region.enter();
        let result = check(source, self.source, fresh);
        drop(scope);
        result
    }

    pub fn declarations(&self, program: &'static Program) -> Self {
        let region = Region::new_scratch(1 << 20);
        let scope = region.enter();
        let source =
            self.source.filter(|_| eligible(program)).map(|source| snapshot(program, Some(source)));
        drop(scope);
        Self { source, region }
    }
}

fn check(
    program: &'static Program,
    snapshot: Option<Snapshot>,
    fresh: impl FnOnce() -> Result<project::ProgramOwner>,
) -> Result<()> {
    let Some(snapshot) = snapshot else {
        return project::check(program, &Context::background());
    };
    let incremental = snapshot.get();
    let context = Context::background();
    let diagnostics = tsrs_compiler::get_diagnostics_of_any_program_like(
        &context,
        &incremental,
        None,
        false,
        &mut |ctx, file| incremental.get_bind_diagnostics(ctx, file),
        &mut |ctx, file| incremental.get_semantic_diagnostics(ctx, file),
    );
    // Incremental diagnostics are grouped per file and include deferred globals
    // in the file cache. Rechecking that Program can stop at those globals and
    // omit file errors. A fresh Program preserves ordinary phase order and text
    // (tests/integration/diagnostic_reuse.rs, deferred_global_errors_match_fresh_checking).

    // TODO: Remove this fallback when a shared API preserves ordinary collection
    // without reusing checker state changed by incremental collection.
    if diagnostics.is_empty() {
        Ok(())
    } else {
        let fresh = fresh()?;
        project::check(fresh.program(), &context)
    }
}

fn snapshot(program: &'static Program, old: Option<Snapshot>) -> Snapshot {
    tsrs_incremental::new_program(
        program,
        old,
        tsrs_incremental::create_host(std::sync::Arc::clone(program.host())),
        None,
        false,
    )
}

/// Syntax anywhere in a Program that restricts diagnostic reuse.
#[derive(Default)]
struct ReuseForms {
    /// UMD declarations (`export as namespace`).
    umd: bool,
    /// Forms whose linking depends on symbol initialization order.
    ordering: bool,
}

impl ReuseForms {
    fn of(program: &Program) -> Self {
        let mut forms = Self::default();
        for file in program.source_files() {
            if forms.scan(file.as_node()) {
                break;
            }
        }
        forms
    }

    /// Record `node`'s forms and its descendants'; returns true once nothing is left to find.
    fn scan(&mut self, node: P<Node>) -> bool {
        // Workaround: snapshot reference collection changes symbol initialization
        // order. The linker orders synthesized imports/namespaces by symbol
        // identity, so these forms keep ordinary checking unless the project is
        // closed and needs neither synthesis path.

        // TODO: Make linker ordering independent of symbol initialization history,
        // then remove this gate. See docs/DTS.md and tests/integration/diagnostic_reuse.rs.
        if ast::is_namespace_export_declaration(node) {
            self.umd = true;
            self.ordering = true;
        } else if ast::is_namespace_import(node)
            || ast::is_namespace_export(node)
            || (ast::is_export_declaration(node)
                && node.as_export_declaration().export_clause().is_none())
        {
            self.ordering = true;
        }
        self.umd || node.for_each_child(&mut |child| self.scan(child))
    }
}

/// Whether diagnostic reuse is supported for this Program.
pub fn eligible(program: &'static Program) -> bool {
    // Pinned Go and tsrs omit UMD global-use edges, so cross-edit incremental
    // diagnostics can stay stale (notes/perf-dts-reuse-gates.md). The bundler keeps
    // this conservative gate; its tests use independent Programs.

    // TODO: Prove parity for source-to-declaration reuse before removing the UMD
    // gate, or revisit when the reference pin tracks these edges. See docs/DTS.md.
    // TODO: Add diagnostic/output parity coverage for project references, skipped
    // library checks and mapper execution before broadening reuse eligibility.
    if !program.command_line().project_references().is_empty()
        || !program.options().skip_lib_check.is_false_or_unknown()
        || !program.options().skip_default_lib_check.is_false_or_unknown()
        || program.options().run_external_code.is_true()
    {
        return false;
    }
    let forms = ReuseForms::of(program);
    !forms.umd && (!forms.ordering || closed_project(program))
}

/// A file of the project itself, not a default or external library.
fn project_file(program: &Program, file: P<ast::SourceFile>) -> bool {
    !program.is_source_file_default_library(file.path())
        && !program.is_source_file_from_external_library(file)
}

fn project_module(program: &Program, file: P<ast::SourceFile>) -> bool {
    project_file(program, file) && ast::is_external_module(file)
}

// Closed project modules link named declarations directly, without synthesizing
// symbol-ordered external imports or module namespaces. Check emitted files too:
// declaration emission can introduce qualified import types absent in the source.
pub fn closed_project(program: &'static Program) -> bool {
    program.source_files().iter().filter(|&&file| project_file(program, file)).all(|&file| {
        ast::is_external_module(file) && !requires_ordering(file.as_node(), file, program)
    })
}

fn internal_module(program: &Program, file: P<ast::SourceFile>, specifier: P<Node>) -> bool {
    ast::is_string_literal_like(specifier)
        && program
            .get_resolved_module_from_module_specifier(file, specifier)
            .and_then(|resolved| program.get_source_file(resolved.resolved_file_name))
            .is_some_and(|target| project_module(program, target))
}

fn requires_ordering(node: P<Node>, file: P<ast::SourceFile>, program: &'static Program) -> bool {
    use ast::Kind;

    if ast::is_namespace_import(node)
        || ast::is_namespace_export(node)
        || ast::is_namespace_export_declaration(node)
        || matches!(node.kind(), Kind::ModuleDeclaration | Kind::ImportEqualsDeclaration)
    {
        return true;
    }
    if node.kind() == Kind::ImportType {
        let import = node.as_import_type_node();
        if !ast::is_literal_import_type_node(node)
            || import.qualifier().is_none()
            || !internal_module(program, file, import.argument().as_literal_type_node().literal())
        {
            return true;
        }
    }
    if matches!(node.kind(), Kind::ImportDeclaration | Kind::ExportDeclaration)
        && let Some(specifier) = ast::get_external_module_name(node)
        && !internal_module(program, file, specifier)
    {
        return true;
    }
    node.for_each_child(&mut |child| requires_ordering(child, file, program))
}
