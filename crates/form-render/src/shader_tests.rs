//! Shader validation and pure-logic tests.
//!
//! Every WGSL module is parsed and validated here, so a broken shader fails
//! `cargo test` instead of the app at runtime.

#[cfg(test)]
mod tests {
    const SHADERS: [(&str, &str); 3] = [
        ("metal", include_str!("wgsl/metal.wgsl")),
        ("heat", include_str!("wgsl/heat.wgsl")),
        ("glass", include_str!("wgsl/glass.wgsl")),
    ];

    #[test]
    fn shaders_parse_and_validate() {
        for (name, source) in SHADERS {
            let module = naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|e| panic!("{name}.wgsl failed to parse: {e:?}"));
            let mut validator = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            );
            validator
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name}.wgsl failed validation: {e:?}"));
        }
    }

    #[test]
    fn blit_parses_and_validates() {
        let module =
            naga::front::wgsl::parse_str(include_str!("wgsl/blit.wgsl")).expect("blit.wgsl parses");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator.validate(&module).expect("blit.wgsl validates");
    }

    /// The uniform struct is shared by every shader; keep its size in step.
    #[test]
    fn uniform_block_is_padded() {
        assert_eq!(std::mem::size_of::<crate::params::GpuUniforms>() % 16, 0);
    }
}
