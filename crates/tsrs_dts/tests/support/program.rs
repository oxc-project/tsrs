use std::sync::Arc;

use tsrs_compiler::{
    Context, Program, ProgramOptions, ProgramRegions, get_diagnostics_of_any_program,
    new_compiler_host, new_program,
};
use tsrs_core::{P, Tristate, arena::Region};
use tsrs_tsoptions::ParseConfigHost;
use tsrs_vfs::FS;

struct ConfigHost(Arc<dyn FS>);
impl ParseConfigHost for ConfigHost {
    fn fs(&self) -> &dyn FS {
        &*self.0
    }
    fn get_current_directory(&self) -> &'static str {
        "/"
    }
}

pub struct HostProgram {
    pub program: &'static Program,
    _region: Region,
}
impl HostProgram {
    pub fn new(fs: Arc<dyn FS>, config_path: &str) -> Self {
        Self::with_config(fs, config_path, |_| {})
    }

    pub fn with_config(
        fs: Arc<dyn FS>,
        config_path: &str,
        configure: impl FnOnce(&mut tsrs_tsoptions::ParsedCommandLine),
    ) -> Self {
        let region = Region::new_scratch(1 << 20);
        let scope = region.enter_scratch();
        let (config, errors) = tsrs_tsoptions::get_parsed_command_line_of_config_file(
            config_path,
            None,
            None,
            &ConfigHost(Arc::clone(&fs)),
            None,
        );
        assert!(errors.is_empty(), "{errors:?}");
        let mut config = config.unwrap();
        configure(&mut config);
        let mut options = ProgramOptions::new(
            P::new(config),
            new_compiler_host(
                &tsrs_core::tspath::get_directory_path(config_path),
                fs,
                &tsrs_vfs::bundled::lib_path(),
                None,
                None,
                None,
            ),
        );
        options.single_threaded = Tristate::True;
        options.canonical_diagnostics = false;
        options.regions = Some(Arc::new(ProgramRegions::default()));
        options.checker_regions = true;
        drop(scope);
        Self { program: new_program(options), _region: region }
    }

    pub fn diagnostics(&self) -> Vec<String> {
        get_diagnostics_of_any_program(
            &Context::background(),
            self.program,
            None,
            false,
            &mut |ctx, file| self.program.get_bind_diagnostics(ctx, file),
            &mut |ctx, file| self.program.get_semantic_diagnostics(ctx, file),
        )
        .iter()
        .map(|diagnostic| format!("TS{}: {}", diagnostic.code(), diagnostic.get()))
        .collect()
    }
}
impl Drop for HostProgram {
    fn drop(&mut self) {
        // SAFETY: this test owns the Program; no update/shared Program exists.
        // Each test drops its checker and diagnostic handles before this owner.
        unsafe { tsrs_compiler::free_unshared_program(self.program) };
    }
}
