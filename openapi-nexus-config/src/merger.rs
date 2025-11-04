//! Configuration merger that combines CLI, env, and config file values

use crate::cli::Commands;
use crate::config::ConfigFile;
use crate::global_config::GlobalConfig;
use crate::typescript_config::TypeScriptConfig;

/// Merge configurations with precedence: CLI > Env > Config File > Defaults
pub struct ConfigMerger;

impl ConfigMerger {
    /// Merge CLI arguments with config file, applying precedence rules
    pub fn merge(
        config_file: Option<&ConfigFile>,
        cli_args: &crate::cli::CliArgs,
    ) -> Result<ResolvedConfig, MergeError> {
        // Extract CLI configs
        let cli_global = match &cli_args.command {
            Commands::Generate { global, .. } => global,
        };

        let cli_typescript = match &cli_args.command {
            Commands::Generate { typescript, .. } => typescript,
        };

        // Extract file configs (if any)
        let file_typescript = config_file.as_ref().map(|f| &f.typescript);

        // Merge global config
        // For non-Option fields with defaults, clap has already set them, so we use CLI/env values.
        // For Option fields, we prefer CLI/env over file.
        let merged_global = GlobalConfig {
            input: cli_global.input.clone(),
            output: cli_global.output.clone(),
            language: cli_global.language.clone(),
            verbose: cli_global.verbose,
        };

        // Merge TypeScript config
        // For non-Option fields with defaults, clap has already set them, so we use CLI/env values.
        // For Option fields, we prefer CLI/env over file.
        let merged_typescript = TypeScriptConfig {
            file_naming_convention: cli_typescript.file_naming_convention.clone(),
            package_scope: cli_typescript
                .package_scope
                .clone()
                .or_else(|| file_typescript.and_then(|c| c.package_scope.clone())),
            package_name: cli_typescript
                .package_name
                .clone()
                .or_else(|| file_typescript.and_then(|c| c.package_name.clone())),
            generate_package: cli_typescript.generate_package,
            ts_target: cli_typescript.ts_target.clone(),
            ts_module: cli_typescript.ts_module.clone(),
            ts_lib: cli_typescript
                .ts_lib
                .clone()
                .or_else(|| file_typescript.and_then(|c| c.ts_lib.clone())),
            generate_esm_config: cli_typescript.generate_esm_config,
            include_build_scripts: cli_typescript.include_build_scripts,
        };

        // Resolve and validate
        let input = merged_global.input.clone();
        let language = merged_global.language.clone();
        let resolved_global = merged_global.resolve(input, language).map_err(|msg| {
            let err = MergeError::ValidationError(msg.clone());
            tracing::error!("{}", err);
            err
        })?;

        Ok(ResolvedConfig {
            global: resolved_global,
            typescript: merged_typescript,
        })
    }
}

/// Resolved configuration with all defaults applied and validations passed
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub global: GlobalConfig,
    pub typescript: TypeScriptConfig,
}

/// Error type for configuration merging
#[derive(Debug)]
pub enum MergeError {
    ValidationError(String),
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MergeError::ValidationError(msg) => {
                write!(f, "Configuration validation error: {}", msg)
            }
        }
    }
}

impl std::error::Error for MergeError {}
