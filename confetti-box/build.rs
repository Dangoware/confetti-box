fn main() {
    #[cfg(feature = "git_tag")]
    {
        let build = vergen_gix::BuildBuilder::all_build().unwrap();
        let cargo = vergen_gix::CargoBuilder::all_cargo().unwrap();
        let gitcl = vergen_gix::GixBuilder::all_git().unwrap();
        let rustc = vergen_gix::RustcBuilder::all_rustc().unwrap();
        let si = vergen_gix::SysinfoBuilder::all_sysinfo().unwrap();

        vergen_gix::Emitter::default()
            .add_instructions(&build)
            .unwrap()
            .add_instructions(&cargo)
            .unwrap()
            .add_instructions(&gitcl)
            .unwrap()
            .add_instructions(&rustc)
            .unwrap()
            .add_instructions(&si)
            .unwrap()
            .emit()
            .unwrap();
    }
}
