// Autonomous Development Agent with Smart Diff Parser, Git Undo & Advanced Model Support
// By LMF Holdings

use chrono::Local;
use clap::Parser;
use colored::*;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;  // Sin Instant

// ═══════════════════════════════════════════════════════════════════════════════
// CONSTANTES DE CONFIGURACIÓN
// ═══════════════════════════════════════════════════════════════════════════════
const APP_NAME: &str = "luismind";
const BRAND: &str = "LUISMIND";
const VERSION: &str = "3.0.0";

// Archivos de configuración
const APIS_CONFIG_FILE: &str = ".config/luismind/apis.conf"; //{APP_NAME}
const KEYS_FILE: &str = ".config/luismind/keys.env"; //{APP_NAME}
const OLLAMA_CONFIG_FILE: &str = ".config/luismind/ollama.conf";

// Alias para compatibilidad (ELIMINAR referencias a LOCAL_APIS_CONFIG_FILE)
const LOCAL_APIS_CONFIG_FILE: &str = ".config/luismind/apis.conf";

// Cooldowns por defecto
const DEFAULT_CLOUD_COOLDOWN: u64 = 60;
const INITIAL_RATE_LIMIT_WAIT: u64 = 30;
const COOLDOWN_MULTIPLIER: f64 = 1.5;
const MAX_COOLDOWN: u64 = 300;

// Límites
const MAX_OUTPUT_LINES: usize = 100_000;
const MAX_SEARCH_REPLACE_BYTES: usize = 500_000;

// Providers locales
const LOCAL_PROVIDERS: &[&str] = &["ollama", "llama-cpp", "oobabooga", "local", "custom", "vllm"];

// ═══════════════════════════════════════════════════════════════════════════════
// PROMPTS - OPTIMIZED FOR COMPLETE SEARCH/REPLACE BLOCKS  at LMF Holdings
// ═══════════════════════════════════════════════════════════════════════════════
const SYSTEM_PROMPT: &str = r#"You are an elite systems engineer. You write flawless, production-grade code.

╔══════════════════════════════════════════════════════════════════════════════╗
║  ⛔ ABSOLUTE PROHIBITION - YOUR RESPONSE WILL BE REJECTED IF YOU USE:       ║
║                                                                              ║
║  • "// ... (omitted lines)"     ← FORBIDDEN - WILL BE REJECTED              ║
║  • "// ... (rest of code)"      ← FORBIDDEN - WILL BE REJECTED              ║
║  • "// ... existing code"       ← FORBIDDEN - WILL BE REJECTED              ║
║  • "// unchanged"               ← FORBIDDEN - WILL BE REJECTED              ║
║  • "// same as before"          ← FORBIDDEN - WILL BE REJECTED              ║
║  • Any form of "..."            ← FORBIDDEN - WILL BE REJECTED              ║
║                                                                              ║
║  YOU MUST SHOW COMPLETE CODE. NO EXCEPTIONS. NO ABBREVIATIONS.              ║
╚══════════════════════════════════════════════════════════════════════════════╝

FOR EVERY FILE CHANGE, use this EXACT format:

path/to/file.rs
<<<<<<< SEARCH
[paste the EXACT complete code block to find - EVERY LINE]
=======
[paste the COMPLETE replacement - EVERY LINE]
>>>>>>> REPLACE

RULES:
1. SEARCH must match EXACTLY what exists in the file
2. REPLACE must contain the COMPLETE new code
3. If a function is 50 lines, show ALL 50 lines
4. If too long, use MULTIPLE SEARCH/REPLACE blocks for different sections
5. NEVER use "omitting unchanged", "rest of the code remains the same" or similar - always show COMPLETE code blocks
6. NEVER leave a partial SEARCH/REPLACE block
7. If running low on response space, STOP and write NEXT_STEPS immediately
8. After EVERY response, include NEXT_STEPS: with numbered remaining tasks
9. Be prolific and anti-lazy - maximize complete output

CORRECT EXAMPLE (function with 3 methods):
impl Calculator {
    fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
    
    fn subtract(&self, a: i32, b: i32) -> i32 {
        a - b
    }
    
    fn multiply(&self, a: i32, b: i32) -> i32 {
        a * b
    }
}

WRONG EXAMPLE (will be REJECTED):
impl Calculator {
    fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
    // ... (rest of methods unchanged)
}

OUTPUT FORMAT - End EVERY response with:
NEXT_STEPS:
1. [file] Task description
(or "None - all tasks complete")

AGENT_HANDOFF:
Technical context for continuation.

BE EXTREMELY PROLIFIC. ANTI-LAZY. COMPLETE CODE ONLY."#;

const TASK_SUFFIX: &str = r#"

══════════════════════════════════════════════════════════════════════════════
FINAL REMINDERS:
• Use <<<<<<< SEARCH / ======= / >>>>>>> REPLACE format
• Show COMPLETE code - NEVER use "...", "omitted", "unchanged", "rest of"
• If you write "// ..." your response will be REJECTED and you must redo
• End with NEXT_STEPS: listing remaining tasks (or "None" if complete)
══════════════════════════════════════════════════════════════════════════════"#;

const CONTINUE_PROMPT: &str = r#"Continue with the PENDING TASKS listed below.

REMEMBER:
- Be EXTREMELY PROLIFIC and ANTI-LAZY
- Show COMPLETE code in every SEARCH/REPLACE block
- NEVER use "..." or "omitted" - this will be REJECTED
- Complete as many tasks as possible in this response"#;

const FIX_COMPILE_PROMPT: &str = r#"⚠️ CRITICAL: COMPILATION ERRORS DETECTED!

You MUST fix these errors IMMEDIATELY before doing anything else:
{errors}

INSTRUCTIONS:
1. Fix ALL compilation errors using complete SEARCH/REPLACE blocks
2. Show the ENTIRE function/block being fixed, not just the changed line
3. NEVER use "..." or "omitted lines" - show COMPLETE code
4. After ALL errors are fixed, proceed with the main task below

MAIN TASK (after fixing errors):
{task}

REMEMBER: Fix errors FIRST, then implement the task. Be prolific!"#;

const EXECUTE_FIRST_PROMPT: &str = r#"⚠️ STEP 1: FIX COMPILATION ERRORS FIRST!

The project has these errors that MUST be fixed before anything else:
{errors}

INSTRUCTIONS:
1. Fix ALL compilation errors using complete SEARCH/REPLACE blocks
2. Show the ENTIRE function/block being fixed, not just the changed line
3. NEVER use "..." or "omitted lines" - show COMPLETE code
4. After ALL errors are fixed, proceed with the main task below

MAIN TASK (after fixing errors):
{task}

REMEMBER: Fix errors FIRST, then implement the task. Be prolific!"#;
const EXAMPLES_SECTION: &str = r#"

═══ EXAMPLES OF CORRECT OUTPUT ═══

EXAMPLE 1 - Small function change:
src/lib.rs
<<<<<<< SEARCH
pub fn calculate_total(items: &[Item]) -> f64 {
    items.iter().map(|i| i.price).sum()
}
=======
pub fn calculate_total(items: &[Item]) -> f64 {
    items.iter()
        .filter(|i| i.is_active)
        .map(|i| i.price * i.quantity as f64)
        .sum()
}
>>>>>>> REPLACE

EXAMPLE 2 - Adding a new function (empty SEARCH):
src/utils.rs
<<<<<<< SEARCH
=======
/// Validates user input
pub fn validate_input(input: &str) -> Result<(), ValidationError> {
    if input.is_empty() {
        return Err(ValidationError::Empty);
    }
    if input.len() > 100 {
        return Err(ValidationError::TooLong);
    }
    Ok(())
}
>>>>>>> REPLACE

EXAMPLE 3 - Modifying impl block (COMPLETE block, not abbreviated):
src/player.rs
<<<<<<< SEARCH
impl Player {
    pub fn new(name: String) -> Self {
        Self { name, score: 0, level: 1 }
    }
    
    pub fn add_score(&mut self, points: i32) {
        self.score += points;
    }
}
=======
impl Player {
    pub fn new(name: String) -> Self {
        Self { name, score: 0, level: 1, health: 100 }
    }
    
    pub fn add_score(&mut self, points: i32) {
        self.score += points;
        if self.score > self.level * 1000 {
            self.level_up();
        }
    }
    
    pub fn level_up(&mut self) {
        self.level += 1;
        self.health = 100;
    }
}
>>>>>>> REPLACE

═══ WRONG (WILL BE REJECTED) ═══

WRONG - Using abbreviation:
impl Player {
    pub fn new(name: String) -> Self {
        Self { name, score: 0, level: 1, health: 100 }
    }
    // ... (rest unchanged)
}

WRONG - Saying "omitted":
impl Player {
    // ... (omitted lines)
}

"#;

const LAZY_PATTERNS: &[&str] = &[
    "// ... (omit",
    "// ...(omit",
    "// ... omit",
    "// ...omit",
    "//(omit",
    "// (omit",
    "// ... (rest",
    "// ...(rest",
    "// ... rest",
    "// existing code",
    "// ... existing",
    "// same as before",
    "// unchanged",
    "// ... unchanged",
    "// keep as is",
    "// no changes",
    "/* ... */",
    "# ... (omit",
    "# ...(omit", 
    "# ... omit",
    "// ...",
    "# ...",
    "...(omit",
    "... (omit",
];
/// Verifica si el output del modelo tiene demasiado código lazy
fn is_output_too_lazy(output: &str) -> bool {
    let lazy_count = output.lines()
        .filter(|l| contains_lazy_pattern(l))
        .count();
    
    // Si más del 5% de las líneas son lazy, rechazar todo
    let total_lines = output.lines().count();
    if total_lines > 20 && lazy_count > total_lines / 20 {
        return true;
    }
    
    // Si hay más de 3 líneas lazy, rechazar
    lazy_count > 3
}

/// Detecta si el contenido tiene patrones lazy
fn contains_lazy_pattern(content: &str) -> bool {
    let lower = content.to_lowercase();
    LAZY_PATTERNS.iter().any(|p| lower.contains(&p.to_lowercase()))
}
/// Detecta líneas específicas con patrones lazy
fn find_lazy_lines(content: &str) -> Vec<usize> {
    let lower_patterns: Vec<String> = LAZY_PATTERNS.iter()
        .map(|p| p.to_lowercase())
        .collect();
    
    content.lines()
        .enumerate()
        .filter(|(_, line)| {
            let lower = line.to_lowercase();
            lower_patterns.iter().any(|p| lower.contains(p))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Intenta reparar un edit que tiene código lazy
/// Reemplaza las líneas lazy con las originales del archivo 
fn repair_lazy_edit(workspace: &Path, edit: &PendingEdit) -> Option<PendingEdit> {
    if !contains_lazy_pattern(&edit.replace) {
        return None;
    }
    
    log_warn(&format!("⚠️ Detectado código lazy en {} - intentando reparar...", edit.filename));
    
    let path = workspace.join(&edit.filename);
    let original = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return None,
    };
    
    let search_trimmed = edit.search.trim();
    if search_trimmed.is_empty() {
        return None;
    }
    
    // Encontrar dónde está el bloque SEARCH en el original
    let search_start = original.find(search_trimmed)?;
    
    // Calcular línea de inicio
    let original_lines: Vec<&str> = original.lines().collect();
    let search_lines: Vec<&str> = edit.search.lines().collect();  // USAR ESTA
    let replace_lines: Vec<&str> = edit.replace.lines().collect();
    
    let mut char_count = 0;
    let mut start_line = 0;
    for (i, line) in original_lines.iter().enumerate() {
        if char_count >= search_start {
            start_line = i;
            break;
        }
        char_count += line.len() + 1;
    }
    
    // Reparar: reemplazar líneas lazy con las originales correspondientes
    let lazy_indices = find_lazy_lines(&edit.replace);
    let mut repaired_lines: Vec<String> = Vec::new();
    
    for (i, line) in replace_lines.iter().enumerate() {
        if lazy_indices.contains(&i) {
            // Buscar línea correspondiente en el SEARCH primero
            if i < search_lines.len() {
                let search_line = search_lines[i];
                if !contains_lazy_pattern(search_line) {
                    repaired_lines.push(search_line.to_string());
                    log_debug(&format!("  Línea {} restaurada desde SEARCH", i + 1), true);
                    continue;
                }
            }
            
            // Si no hay en SEARCH, buscar en original
            let original_idx = start_line + i;
            if original_idx < original_lines.len() {
                let orig_line = original_lines[original_idx];
                if !contains_lazy_pattern(orig_line) {
                    repaired_lines.push(orig_line.to_string());
                    log_debug(&format!("  Línea {} restaurada desde original", i + 1), true);
                    continue;
                }
            }
            
            // No se pudo reparar esta línea
            repaired_lines.push("// TODO: COMPLETE THIS - was lazy code".to_string());
        } else {
            repaired_lines.push(line.to_string());
        }
    }
    
    let repaired_replace = repaired_lines.join("\n");
    
    if contains_lazy_pattern(&repaired_replace) {
        log_warn("  Reparación incompleta - aún hay código lazy");
        return None;
    }
    
    log_ok(&format!("  ✓ Reparadas {} líneas lazy", lazy_indices.len()));
    
    Some(PendingEdit {
        filename: edit.filename.clone(),
        search: edit.search.clone(),
        replace: repaired_replace,
    })
}
/// Intenta reparación más agresiva: copiar estructura del SEARCH al REPLACE

fn repair_lazy_edit_aggressive(workspace: &Path, edit: &PendingEdit) -> Option<PendingEdit> {
    if !contains_lazy_pattern(&edit.replace) {
        return None;
    }
    
    log_warn(&format!("⚠️ Reparación agresiva para {} ...", edit.filename));
    
    let path = workspace.join(&edit.filename);
    let original = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return None,
    };
    
    let search_lines: Vec<&str> = edit.search.lines().collect();
    let replace_lines: Vec<&str> = edit.replace.lines().collect();
    let original_lines: Vec<&str> = original.lines().collect();  // USAR ESTA
    
    // Buscar dónde está el SEARCH en el original
    let search_trimmed = edit.search.trim();
    let search_pos = original.find(search_trimmed).unwrap_or(0);
    
    // Calcular línea de inicio en el original
    let mut start_line_in_original = 0;
    let mut char_count = 0;
    for (i, line) in original_lines.iter().enumerate() {
        if char_count >= search_pos {
            start_line_in_original = i;
            break;
        }
        char_count += line.len() + 1;
    }
    
    // Estrategia agresiva: reconstruir REPLACE usando SEARCH y original
    let mut result_lines: Vec<String> = Vec::new();
    let mut search_idx = 0;
    
    for (replace_idx, replace_line) in replace_lines.iter().enumerate() {
        if contains_lazy_pattern(replace_line) {
            // Línea lazy - buscar en SEARCH primero
            if search_idx < search_lines.len() {
                let search_line = search_lines[search_idx];
                if !contains_lazy_pattern(search_line) {
                    result_lines.push(search_line.to_string());
                    search_idx += 1;
                    continue;
                }
            }
            
            // Buscar en original
            let orig_idx = start_line_in_original + replace_idx;
            if orig_idx < original_lines.len() {
                result_lines.push(original_lines[orig_idx].to_string());
            } else {
                result_lines.push("// TODO: COMPLETE".to_string());
            }
        } else {
            result_lines.push(replace_line.to_string());
            
            // Intentar mantener sincronización con SEARCH
            if search_idx < search_lines.len() {
                search_idx += 1;
            }
        }
    }
    
    let repaired = result_lines.join("\n");
    
    if contains_lazy_pattern(&repaired) || repaired.trim().is_empty() {
        log_warn("  Reparación agresiva falló - aún hay código lazy");
        return None;
    }
    
    log_ok(&format!("  ✓ Reparación agresiva exitosa ({} líneas)", result_lines.len()));
    
    Some(PendingEdit {
        filename: edit.filename.clone(),
        search: edit.search.clone(),
        replace: repaired,
    })
}
// ═══════════════════════════════════════════════════════════════════════════════
// CTRL+C HANDLER - GRACEFUL SHUTDOWN
// ═══════════════════════════════════════════════════════════════════════════════

static CTRLC_COUNT: AtomicUsize = AtomicUsize::new(0);
static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);

fn setup_ctrlc_handler() {
    let _ = ctrlc::set_handler(move || {
        let count = CTRLC_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
        SHOULD_EXIT.store(true, Ordering::SeqCst);
        eprintln!();
        if count >= 2 {
            eprintln!("{}", "⚡ Forced exit!".red().bold());
            kill_all_aider();
            process::exit(130);
        } else {
            eprintln!("{}", "💀 Ctrl+C - terminating gracefully...".yellow());
        }
    });
}

fn kill_all_aider() {
    let _ = Command::new("pkill").args(["-9", "-f", "aider"]).output();
    let _ = Command::new("pkill").args(["-9", "-f", "litellm"]).output();
}

fn should_exit() -> bool {
    SHOULD_EXIT.load(Ordering::SeqCst)
}
// ═══════════════════════════════════════════════════════════════════════════════
// ═══════════════════════════════════════════════════════════════════════════════
// CONFIGURACIÓN DE APIS LOCALES
// ═══════════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════════
// ESTRUCTURA DE MODELO DINÁMICA
// ═══════════════════════════════════════════════════════════════════════════════
// ═══════════════════════════════════════════════════════════════════════════════
// ESTRUCTURAS BASE - DISEÑO SINÉRGICO
// ═══════════════════════════════════════════════════════════════════════════════

/// Modelo dinámico - contiene toda la info necesaria para usarlo
#[derive(Debug, Clone)]
struct DynamicModel {
    /// Nombre completo con prefijo (ej: "gemini/gemini-2.5-pro", "ollama/qwen-coder")
    /// Este es el que el usuario escribe en -m
    name: String,
    
    /// ID para aider (ej: "gemini/gemini-2.5-pro", "ollama_chat/qwen-coder")
    /// Se construye con aider_model_prefix del config
    id: String,
    
    /// Nombre para mostrar al usuario
    display_name: String,
    
    /// Nombre del proveedor (debe coincidir con [nombre] en apis.conf)
    provider: String,
    
    /// Límite de tokens de contexto
    token_limit: usize,
    
    /// Si es gratuito
    is_free: bool,
    
    /// Cooldown en segundos (heredado del provider)
    cooldown_time: u64,
}


impl DynamicModel {
    /// Crea un modelo desde un ApiConfig - ÚNICA FORMA DE CREAR MODELOS
    /// Esto garantiza que todos los campos se deriven correctamente del config
    fn from_api_config(
        raw_model_name: &str,
        api_config: &ApiConfig,
        context_override: Option<usize>,
    ) -> Self {
        // Construir nombre con prefijo (lo que el usuario escribe)
        let name = api_config.build_user_model_name(raw_model_name);
        
        // Construir ID para aider
        let id = api_config.build_aider_model_id(raw_model_name);
        
        // Display name
        let display_name = format!("{} ({})", raw_model_name, api_config.name);
        
        // Contexto: override > inferido > default del config
        let token_limit = context_override
            .unwrap_or_else(|| {
                let inferred = infer_context_from_model_name(raw_model_name);
                inferred.max(api_config.default_context)
            });
        
        Self {
            name,
            id,
            display_name,
            provider: api_config.name.clone(),
            token_limit,
            is_free: api_config.is_local(),
            cooldown_time: api_config.cooldown_time,
        }
    }
    
    /// Nombre sin prefijo (el nombre "raw" del modelo)
    fn raw_name(&self) -> &str {
        if let Some(pos) = self.name.find('/') {
            &self.name[pos + 1..]
        } else {
            &self.name
        }
    }
    
    /// Verifica si este modelo coincide con un nombre dado
    fn matches(&self, query: &str) -> bool {
        let query_lower = query.to_lowercase();
        let name_lower = self.name.to_lowercase();
        let raw_lower = self.raw_name().to_lowercase();
        let id_lower = self.id.to_lowercase();
        
        // Match exacto
        if name_lower == query_lower || raw_lower == query_lower || id_lower == query_lower {
            return true;
        }
        
        // Match parcial
        name_lower.contains(&query_lower) || 
        raw_lower.contains(&query_lower) ||
        id_lower.contains(&query_lower)
    }
}

#[derive(Debug, Clone)]
struct LocalModelConfig {
    name: String,
    display_name: String,
    context: usize,
}

#[derive(Debug, Clone)]
struct LocalApiConfig {
    name: String,
    provider: String,
    host: String,
    port: u16,
    api_base: String,
    api_key: String,
    default_context: usize,
    enabled: bool,
    models: Vec<LocalModelConfig>,
    // AGREGAR ESTOS CAMPOS:
    prefix: String,
    aider_model_prefix: String,
}

impl Default for LocalApiConfig {
    fn default() -> Self {
        Self {
            name: "local".to_string(),
            provider: "local".to_string(),
            host: "127.0.0.1".to_string(),
            port: 5000,
            api_base: String::new(),
            api_key: "sk-dummy".to_string(),
            default_context: 131072,
            enabled: true,
            models: Vec::new(),
            prefix: "local".to_string(),
            aider_model_prefix: "openai/".to_string(),
        }
    }
}
// ═══════════════════════════════════════════════════════════════════════════════
// CONFIGURACIÓN DE API - CENTRALIZA TODO
// ═══════════════════════════════════════════════════════════════════════════════

/// Tipo de API
#[derive(Debug, Clone, PartialEq, Eq)]
enum ApiType {
    Local,
    Cloud,
}

/// Tipo de autenticación
#[derive(Debug, Clone)]
enum AuthType {
    Bearer,          // Authorization: Bearer <key>
    XApiKey,         // x-api-key: <key>
    Query(String),   // ?param=<key>
    None,
}

impl Default for AuthType {
    fn default() -> Self {
        AuthType::Bearer
    }
}

/// Configuración completa de una API
#[derive(Debug, Clone)]
struct ApiConfig {
    // ═══ Identificación ═══
    /// Nombre único (coincide con [nombre] en config)
    name: String,
    /// Tipo de API
    api_type: ApiType,
    /// Habilitada
    enabled: bool,
    
    // ═══ Conexión (Local) ═══
    host: String,
    port: u16,
    models_endpoint: String,
    models_command: Option<String>,
    
    // ═══ Conexión (Cloud) ═══
    env_key: String,
    models_url: String,
    auth_type: AuthType,
    
    // ═══ Construcción de Nombres ═══
    /// Prefijo para el usuario (ej: "gemini", "ollama")
    /// El modelo se muestra como prefix/model_name
    prefix: String,
    /// Prefijo para aider (ej: "gemini/", "ollama_chat/", "openai/")
    aider_model_prefix: String,
    
    // ═══ Defaults ═══
    default_context: usize,
    
    // ═══ Aider Config ═══
    aider_env: String,
    aider_api_base: Option<String>,
    aider_api_key: Option<String>,
    
    // ═══ Rate Limiting ═══
    /// Cooldown base después de error (0 = sin espera, típico para locales)
    cooldown_time: u64,
    /// Cooldown actual (se incrementa con backoff)
    current_cooldown: u64,
    /// Máximo cooldown
    max_cooldown: u64,
    /// Factor de backoff exponencial
    cooldown_multiplier: f64,
    /// Tiempo entre requests exitosos (para no saturar)
    request_interval: u64,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            api_type: ApiType::Cloud,
            enabled: true,
            
            host: "127.0.0.1".to_string(),
            port: 8080,
            models_endpoint: "/v1/models".to_string(),
            models_command: None,
            
            env_key: String::new(),
            models_url: String::new(),
            auth_type: AuthType::Bearer,
            
            prefix: String::new(),
            aider_model_prefix: String::new(),
            
            default_context: 32768,
            
            aider_env: String::new(),
            aider_api_base: None,
            aider_api_key: None,
            
            cooldown_time: 60,  // Cloud default
            current_cooldown: 0,
            max_cooldown: 300,
            cooldown_multiplier: 1.5,
            request_interval: 1,
        }
    }
}

impl ApiConfig {
    // ═══════════════════════════════════════════════════════════════
    // BUILDERS DE NOMBRES - CENTRALIZADO
    // ═══════════════════════════════════════════════════════════════
    
    /// Construye el nombre que el usuario ve/escribe
    /// Formato: prefix/model_name (ej: "gemini/gemini-2.5-pro")
    fn build_user_model_name(&self, raw_model_name: &str) -> String {
        let prefix = if self.prefix.is_empty() { &self.name } else { &self.prefix };
        
        // Evitar doble prefijo (gemini/gemini-2.5-pro ya tiene gemini)
        let raw_lower = raw_model_name.to_lowercase();
        let prefix_lower = prefix.to_lowercase();
        
        if raw_lower.starts_with(&prefix_lower) {
            // El modelo ya incluye el prefijo en su nombre
            format!("{}/{}", prefix, raw_model_name)
        } else {
            format!("{}/{}", prefix, raw_model_name)
        }
    }
    
    /// Construye el ID que aider necesita
    /// Usa aider_model_prefix del config
    fn build_aider_model_id(&self, raw_model_name: &str) -> String {
        if self.aider_model_prefix.is_empty() {
            // Sin prefijo especial, usar el nombre tal cual
            raw_model_name.to_string()
        } else {
            format!("{}{}", self.aider_model_prefix, raw_model_name)
        }
    }
    
    // ═══════════════════════════════════════════════════════════════
    // UTILIDADES
    // ═══════════════════════════════════════════════════════════════
    
    /// Es API local
    fn is_local(&self) -> bool {
        self.api_type == ApiType::Local
    }
    
    /// URL base para API local
    fn get_local_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
    
    /// Verifica disponibilidad
    fn is_available(&self, keys: &HashMap<String, String>) -> bool {
        match self.api_type {
            ApiType::Local => is_port_open(&self.host, self.port),
            ApiType::Cloud => self.get_api_key(keys).is_some(),
        }
    }
    
    /// Verifica disponibilidad con logging
    fn check_availability(&self, keys: &HashMap<String, String>, verbose: bool) -> bool {
        let available = self.is_available(keys);
        
        if verbose || available {
            match self.api_type {
                ApiType::Local => {
                    if available {
                        log_ok(&format!("  ✓ {} disponible ({}:{})", 
                            self.name, self.host, self.port));
                    } else {
                        log_debug(&format!("  ✗ {} no disponible ({}:{})", 
                            self.name, self.host, self.port), verbose);
                    }
                }
                ApiType::Cloud => {
                    if available {
                        log_ok(&format!("  ✓ {} API key encontrada ({})", 
                            self.name, mask_env_key(&self.env_key)));
                    } else {
                        log_debug(&format!("  ✗ {} sin API key ({})", 
                            self.name, self.env_key), verbose);
                    }
                }
            }
        }
        
        available
    }
    
    /// Obtiene API key
    fn get_api_key(&self, keys: &HashMap<String, String>) -> Option<String> {
        // Del HashMap primero
        if let Some(key) = keys.get(&self.env_key) {
            if !key.is_empty() {
                return Some(key.clone());
            }
        }
        // Del entorno
        if let Ok(key) = env::var(&self.env_key) {
            if !key.is_empty() {
                return Some(key);
            }
        }
        None
    }
    
    // ═══════════════════════════════════════════════════════════════
    // RATE LIMITING
    // ═══════════════════════════════════════════════════════════════
    
    /// Obtiene el cooldown actual (0 para locales)
    fn get_cooldown(&self) -> u64 {
        if self.is_local() {
            0
        } else {
            if self.current_cooldown > 0 {
                self.current_cooldown
            } else {
                self.cooldown_time
            }
        }
    }
    
    /// Incrementa cooldown (backoff exponencial)
    fn increment_cooldown(&mut self) {
        if self.is_local() {
            return;
        }
        
        self.current_cooldown = if self.current_cooldown == 0 {
            self.cooldown_time
        } else {
            let new_val = (self.current_cooldown as f64 * self.cooldown_multiplier) as u64;
            new_val.min(self.max_cooldown)
        };
    }
    
    /// Resetea cooldown después de éxito
    fn reset_cooldown(&mut self) {
        self.current_cooldown = 0;
    }
    
    /// Espera el cooldown si es necesario
    fn wait_cooldown(&self) {
        let wait_time = self.get_cooldown();
        if wait_time > 0 {
            wait_with_countdown(wait_time, &format!("cooldown de {}", self.name));
        }
    }
    
    // ═══════════════════════════════════════════════════════════════
    // AIDER
    // ═══════════════════════════════════════════════════════════════
    
    /// Configura el comando aider para usar esta API
    fn configure_aider(&self, cmd: &mut Command, keys: &HashMap<String, String>) {
        match self.api_type {
            ApiType::Local => {
                // API base
                let base = self.aider_api_base.clone()
                    .unwrap_or_else(|| format!("{}/v1", self.get_local_url()));
                cmd.args(["--openai-api-base", &base]);
                
                // API key
                let key = self.aider_api_key.clone().unwrap_or_else(|| "sk-dummy".to_string());
                cmd.args(["--openai-api-key", &key]);
                
                // Config especial para Ollama
                if self.name == "ollama" {
                    for (k, v) in get_ollama_env_vars_optimized("") {
                        cmd.env(&k, &v);
                    }
                }
            }
            ApiType::Cloud => {
                if !self.aider_env.is_empty() {
                    if let Some(key) = self.get_api_key(keys) {
                        cmd.env(&self.aider_env, &key);
                    }
                }
            }
        }
    }
    
    // ═══════════════════════════════════════════════════════════════
    // FETCH MODELS
    // ═══════════════════════════════════════════════════════════════
    
    /// Obtiene modelos de esta API
    fn fetch_models(&self, keys: &HashMap<String, String>) -> Vec<DynamicModel> {
        match self.api_type {
            ApiType::Local => self.fetch_local_models(),
            ApiType::Cloud => self.fetch_cloud_models(keys),
        }
    }
    
    /// Modelos de API local
    fn fetch_local_models(&self) -> Vec<DynamicModel> {
        // Si hay comando especial (ollama list)
        if let Some(ref cmd) = self.models_command {
            return self.fetch_models_via_command(cmd);
        }
        
        // HTTP endpoint
        let url = format!("{}{}", self.get_local_url(), self.models_endpoint);
        self.fetch_models_from_url(&url, None)
    }
    
    /// Modelos de API cloud
    fn fetch_cloud_models(&self, keys: &HashMap<String, String>) -> Vec<DynamicModel> {
        let api_key = match self.get_api_key(keys) {
            Some(k) => k,
            None => return Vec::new(),
        };
        
        self.fetch_models_from_url(&self.models_url, Some(&api_key))
    }
    
    /// Fetch via comando (ej: ollama list)
    fn fetch_models_via_command(&self, cmd_str: &str) -> Vec<DynamicModel> {
        let parts: Vec<&str> = cmd_str.split_whitespace().collect();
        if parts.is_empty() {
            return Vec::new();
        }
        
        let output = Command::new(parts[0])
            .args(&parts[1..])
            .output();
        
        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return Vec::new(),
        };
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        self.parse_ollama_list_output(&stdout)
    }
    
    /// Parsea output de "ollama list"
    fn parse_ollama_list_output(&self, output: &str) -> Vec<DynamicModel> {
        let mut models = Vec::new();
        
        for line in output.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }
            
            let raw_name = parts[0];
            let size_info = parts.get(2).map(|s| *s).unwrap_or("");
            
            let mut model = DynamicModel::from_api_config(raw_name, self, None);
            
            // Mejorar display_name con tamaño
            if !size_info.is_empty() {
                model.display_name = format!("{} ({}) [{}]", raw_name, self.name, size_info);
            }
            
            models.push(model);
        }
        
        models
    }
    
    /// Fetch via HTTP (curl)
    fn fetch_models_from_url(&self, url: &str, api_key: Option<&str>) -> Vec<DynamicModel> {
        let mut args = vec!["-s".to_string(), "-m".to_string(), "15".to_string()];
        
        // Autenticación
        let final_url = match (&self.auth_type, api_key) {
            (AuthType::Bearer, Some(key)) => {
                args.push("-H".to_string());
                args.push(format!("Authorization: Bearer {}", key));
                url.to_string()
            }
            (AuthType::XApiKey, Some(key)) => {
                args.push("-H".to_string());
                args.push(format!("x-api-key: {}", key));
                url.to_string()
            }
            (AuthType::Query(param), Some(key)) => {
                if url.contains('?') {
                    format!("{}&{}={}", url, param, key)
                } else {
                    format!("{}?{}={}", url, param, key)
                }
            }
            _ => url.to_string(),
        };
        
        args.push(final_url);
        
        let output = Command::new("curl")
            .args(&args)
            .output();
        
        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return Vec::new(),
        };
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        self.parse_models_json(&stdout)
    }
    
    /// Parsea JSON de modelos
    fn parse_models_json(&self, json: &str) -> Vec<DynamicModel> {
        let mut models = Vec::new();
        
        // Detectar formato (Gemini usa "models", otros usan "data")
        let is_gemini = json.contains("\"models\"");
        let id_field = if is_gemini { "name" } else { "id" };
        
        let mut pos = 0;
        while let Some(field_pos) = json[pos..].find(&format!("\"{}\"", id_field)) {
            let abs_pos = pos + field_pos;
            pos = abs_pos + id_field.len() + 2;
            
            // Buscar el valor
            if let Some(colon) = json[pos..].find(':') {
                let value_start = pos + colon + 1;
                
                if let Some(q1) = json[value_start..].find('"') {
                    let name_start = value_start + q1 + 1;
                    
                    if let Some(q2) = json[name_start..].find('"') {
                        let raw_name = &json[name_start..name_start + q2];
                        
                        // Validar
                        if raw_name.is_empty() || raw_name.len() > 200 ||
                           raw_name.contains('{') || raw_name.contains('}') {
                            continue;
                        }
                        
                        // Para Gemini, limpiar "models/" prefix
                        let clean_name = if is_gemini && raw_name.starts_with("models/") {
                            &raw_name[7..]
                        } else {
                            raw_name
                        };
                        
                        // Filtrar modelos no relevantes
                        if self.should_skip_model(clean_name) {
                            continue;
                        }
                        
                        let model = DynamicModel::from_api_config(clean_name, self, None);
                        
                        // Evitar duplicados
                        if !models.iter().any(|m: &DynamicModel| m.name == model.name) {
                            models.push(model);
                        }
                        
                        pos = name_start + q2 + 1;
                    }
                }
            }
        }
        
        models
    }
    
    /// Filtrar modelos no relevantes para coding
    fn should_skip_model(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        
        // Embedding models
        if lower.contains("embed") && !lower.contains("code") {
            return true;
        }
        
        // Audio/image models
        if lower.contains("whisper") || lower.contains("tts") || 
           lower.contains("dall-e") || lower.contains("imagen") {
            return true;
        }
        
        // Moderation
        if lower.contains("moderation") {
            return true;
        }
        
        // Muy viejos
        if lower.contains("davinci") || lower.contains("curie") || 
           lower.contains("babbage") || lower.contains("ada") {
            if !lower.contains("gpt") {
                return true;
            }
        }
        
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════════

/// Verifica si un puerto está abierto
fn is_port_open(host: &str, port: u16) -> bool {
    use std::net::TcpStream;
    let addr = format!("{}:{}", host, port);
    TcpStream::connect_timeout(
        &addr.parse().unwrap_or_else(|_| "127.0.0.1:80".parse().unwrap()),
        Duration::from_secs(2)
    ).is_ok()
}

/// Máscara para mostrar env key
fn mask_env_key(env_key: &str) -> String {
    if env_key.len() > 10 {
        format!("{}...{}", &env_key[..6], &env_key[env_key.len()-4..])
    } else {
        env_key.to_string()
    }
}

/// Inferir contexto del nombre del modelo
fn infer_context_from_model_name(name: &str) -> usize {
    let lower = name.to_lowercase();
    
    // Patrones explícitos
    if lower.contains("1m") || lower.contains("1000k") { return 1_048_576; }
    if lower.contains("512k") { return 524_288; }
    if lower.contains("256k") { return 262_144; }
    if lower.contains("200k") { return 204_800; }
    if lower.contains("128k") { return 131_072; }
    if lower.contains("64k") { return 65_536; }
    if lower.contains("32k") { return 32_768; }
    if lower.contains("16k") { return 16_384; }
    if lower.contains("8k") { return 8_192; }
    if lower.contains("4k") { return 4_096; }
    
    // Por familia de modelo
    if lower.contains("gemini-2") || lower.contains("gemini-exp") { return 1_048_576; }
    if lower.contains("gemini-1.5") { return 1_048_576; }
    if lower.contains("gemini") { return 32_768; }
    if lower.contains("claude-3.5") || lower.contains("claude-3-5") { return 200_000; }
    if lower.contains("claude-3") { return 200_000; }
    if lower.contains("claude") { return 100_000; }
    if lower.contains("gpt-4o") || lower.contains("gpt-4-turbo") { return 128_000; }
    if lower.contains("gpt-4") { return 8_192; }
    if lower.contains("gpt-3.5") { return 16_384; }
    if lower.contains("qwen3") || lower.contains("qwen-3") { return 131_072; }
    if lower.contains("qwen2.5") || lower.contains("qwen-2.5") { return 131_072; }
    if lower.contains("qwen") { return 32_768; }
    if lower.contains("llama-3.3") || lower.contains("llama3.3") { return 131_072; }
    if lower.contains("llama-3.2") || lower.contains("llama3.2") { return 131_072; }
    if lower.contains("llama-3.1") || lower.contains("llama3.1") { return 131_072; }
    if lower.contains("llama-3") || lower.contains("llama3") { return 8_192; }
    if lower.contains("llama") { return 4_096; }
    if lower.contains("deepseek-v3") { return 131_072; }
    if lower.contains("deepseek-coder") { return 131_072; }
    if lower.contains("deepseek") { return 65_536; }
    if lower.contains("phi-3") || lower.contains("phi3") { return 131_072; }
    if lower.contains("phi-4") || lower.contains("phi4") { return 131_072; }
    if lower.contains("command-r") { return 131_072; }
    if lower.contains("mixtral") { return 32_768; }
    if lower.contains("mistral-large") { return 131_072; }
    if lower.contains("mistral") { return 32_768; }
    if lower.contains("codestral") { return 32_768; }
    if lower.contains("gemma-2") { return 8_192; }
    if lower.contains("gemma") { return 8_192; }
    
    // Default conservador
    32_768
}

// ═══════════════════════════════════════════════════════════════════════════════
// CARGA DE CONFIGURACIÓN
// ═══════════════════════════════════════════════════════════════════════════════

/// Carga todas las configuraciones de APIs
fn load_all_api_configs() -> Vec<ApiConfig> {
    let mut configs = Vec::new();
    
    // Intentar cargar desde archivo
    if let Some(home) = dirs::home_dir() {
        let config_path = home.join(APIS_CONFIG_FILE);
        
        if config_path.exists() {
            log_debug(&format!("Cargando APIs desde {}", config_path.display()), true);
            configs = parse_apis_config_file(&config_path);
        }
    }
    
    // Si no hay configs, usar defaults
    if configs.is_empty() {
        log_debug("Usando configuración de APIs por defecto", true);
        configs = get_default_api_configs();
    }
    
    configs
}

/// Parsea archivo de configuración
fn parse_apis_config_file(path: &PathBuf) -> Vec<ApiConfig> {
    let mut configs = Vec::new();
    
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            log_warn(&format!("Error leyendo {}: {}", path.display(), e));
            return configs;
        }
    };
    
    let mut current: Option<ApiConfig> = None;
    
    for line in content.lines() {
        let line = line.trim();
        
        // Ignorar comentarios y vacías
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        
        // Nueva sección [nombre]
        if line.starts_with('[') && line.ends_with(']') {
            // Guardar anterior
            if let Some(cfg) = current.take() {
                configs.push(cfg);
            }
            
            let name = line.trim_matches(|c| c == '[' || c == ']').to_string();
            current = Some(ApiConfig {
                name: name.clone(),
                prefix: name.clone(),
                ..Default::default()
            });
            continue;
        }
        
        // Key = Value
        if let Some(pos) = line.find('=') {
            let key = line[..pos].trim().to_lowercase();
            let value = line[pos + 1..].trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            
            if let Some(ref mut cfg) = current {
                apply_config_value(cfg, &key, &value);
            }
        }
    }
    
    // Guardar última
    if let Some(cfg) = current {
        configs.push(cfg);
    }
    
    configs
}

/// Aplica un valor de configuración
fn apply_config_value(cfg: &mut ApiConfig, key: &str, value: &str) {
    match key {
        // Tipo
        "type" => {
            cfg.api_type = if value == "local" { ApiType::Local } else { ApiType::Cloud };
            // Locales no tienen cooldown por defecto
            if cfg.api_type == ApiType::Local {
                cfg.cooldown_time = 0;
                cfg.max_cooldown = 0;
            }
        }
        "enabled" => {
            cfg.enabled = matches!(value, "true" | "1" | "yes" | "on");
        }
        
        // Conexión local
        "host" => cfg.host = value.to_string(),
        "port" => cfg.port = value.parse().unwrap_or(8080),
        "models_endpoint" => cfg.models_endpoint = value.to_string(),
        "models_command" => cfg.models_command = Some(value.to_string()),
        
        // Conexión cloud
        "env_key" => cfg.env_key = value.to_string(),
        "models_url" => cfg.models_url = value.to_string(),
        "auth_type" => {
            cfg.auth_type = match value {
                "bearer" => AuthType::Bearer,
                "x-api-key" => AuthType::XApiKey,
                "query" => AuthType::Query("key".to_string()),
                "none" => AuthType::None,
                _ => AuthType::Bearer,
            };
        }
        "auth_param" => {
            if matches!(cfg.auth_type, AuthType::Query(_)) {
                cfg.auth_type = AuthType::Query(value.to_string());
            }
        }
        
        // Nombres
       "prefix" => cfg.prefix = value.to_string(),
        "aider_model_prefix" | "aider_prefix" => cfg.aider_model_prefix = value.to_string(),
        
        // Defaults
        "default_context" | "context" => {
            cfg.default_context = parse_context_value(value);
        }
        
        // Aider
        "aider_env" => cfg.aider_env = value.to_string(),
        "aider_api_base" => cfg.aider_api_base = Some(value.to_string()),
        "aider_api_key" => cfg.aider_api_key = Some(value.to_string()),
        
        // Rate limiting
        "cooldown_time" | "cooldown" => {
            cfg.cooldown_time = value.parse().unwrap_or(60);
        }
        "max_cooldown" => {
            cfg.max_cooldown = value.parse().unwrap_or(300);
        }
        "cooldown_multiplier" | "backoff" | "backoff_multiplier" => {
            cfg.cooldown_multiplier = value.parse().unwrap_or(1.5);
        }
        "request_interval" | "interval" => {
            cfg.request_interval = value.parse().unwrap_or(1);
        }
        
        _ => {}
    }
}

/// Parsea valores de contexto (128k, 1M, etc.)
fn parse_context_value(value: &str) -> usize {
    let lower = value.to_lowercase();
    let trimmed = lower.trim();
    
    if trimmed.ends_with('k') {
        let num: usize = trimmed.trim_end_matches('k').trim().parse().unwrap_or(32);
        num * 1024
    } else if trimmed.ends_with('m') {
        let num: usize = trimmed.trim_end_matches('m').trim().parse().unwrap_or(1);
        num * 1024 * 1024
    } else {
        value.trim().parse().unwrap_or(32768)
    }
}

/// Configuraciones por defecto
fn get_default_api_configs() -> Vec<ApiConfig> {
    vec![
        // Ollama
        ApiConfig {
            name: "ollama".to_string(),
            api_type: ApiType::Local,
            host: "127.0.0.1".to_string(),
            port: 11434,
            models_command: Some("ollama list".to_string()),
            prefix: "ollama".to_string(),
            aider_model_prefix: "ollama_chat/".to_string(),
            default_context: 32768,
            cooldown_time: 0,
            max_cooldown: 0,
            enabled: true,
            ..Default::default()
        },
        // Gemini
        ApiConfig {
            name: "gemini".to_string(),
            api_type: ApiType::Cloud,
            env_key: "GEMINI_API_KEY".to_string(),
            models_url: "https://generativelanguage.googleapis.com/v1beta/models".to_string(),
            auth_type: AuthType::Query("key".to_string()),
            prefix: "gemini".to_string(),
            aider_model_prefix: "gemini/".to_string(),
            aider_env: "GEMINI_API_KEY".to_string(),
            default_context: 1_000_000,
            cooldown_time: 30,
            max_cooldown: 300,
            cooldown_multiplier: 1.5,
            enabled: true,
            ..Default::default()
        },
        // Groq
        ApiConfig {
            name: "groq".to_string(),
            api_type: ApiType::Cloud,
            env_key: "GROQ_API_KEY".to_string(),
            models_url: "https://api.groq.com/openai/v1/models".to_string(),
            auth_type: AuthType::Bearer,
            prefix: "groq".to_string(),
            aider_model_prefix: "groq/".to_string(),
            aider_env: "GROQ_API_KEY".to_string(),
            default_context: 131072,
            cooldown_time: 30,
            max_cooldown: 120,
            cooldown_multiplier: 2.0,
            enabled: true,
            ..Default::default()
        },
    ]
}

/// Carga API keys desde archivo y entorno
fn load_api_keys() -> HashMap<String, String> {
    let mut keys = HashMap::new();
    
    // Cargar desde archivo
    if let Some(home) = dirs::home_dir() {
        for filename in &["keys.env", ".env"] {
            let keys_path = home.join(".config/luismind").join(filename);
            
            if keys_path.exists() {
                if let Ok(content) = fs::read_to_string(&keys_path) {
                    for line in content.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }
                        
                        if let Some(pos) = line.find('=') {
                            let key = line[..pos].trim().to_string();
                            let value = line[pos + 1..].trim()
                                .trim_matches('"')
                                .trim_matches('\'')
                                .to_string();
                            
                            if !value.is_empty() {
                                keys.insert(key, value);
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Override con variables de entorno
    let env_keys = [
        "GEMINI_API_KEY", "GROQ_API_KEY", "DEEPSEEK_API_KEY",
        "OPENROUTER_API_KEY", "TOGETHER_API_KEY", "FIREWORKS_API_KEY",
        "ANTHROPIC_API_KEY", "OPENAI_API_KEY", "COHERE_API_KEY",
        "MISTRAL_API_KEY", "CEREBRAS_API_KEY", "SAMBANOVA_API_KEY",
        "NOVITA_API_KEY", "HYPERBOLIC_API_KEY", "CHUTES_API_KEY",
        "HUGGINGFACE_API_KEY", "HF_TOKEN",
    ];
    
    for key_name in env_keys {
        if let Ok(value) = env::var(key_name) {
            if !value.is_empty() {
                keys.insert(key_name.to_string(), value);
            }
        }
    }
    
    keys
}

/// Encuentra config de API por nombre
fn find_api_config(name: &str) -> Option<ApiConfig> {
    let configs = load_all_api_configs();
    let name_lower = name.to_lowercase();
    
    configs.into_iter().find(|c| {
        c.name.to_lowercase() == name_lower ||
        c.prefix.to_lowercase() == name_lower
    })
}

/// Encuentra config de API para un provider
fn find_api_config_for_provider(provider: &str) -> Option<ApiConfig> {
    find_api_config(provider)
}

// ═══════════════════════════════════════════════════════════════════════════════
// OBTENCIÓN UNIFICADA DE MODELOS
// ═══════════════════════════════════════════════════════════════════════════════

/// Obtiene todos los modelos de todas las APIs disponibles
fn get_all_models_unified(verbose: bool) -> Vec<DynamicModel> {
    let mut all_models = Vec::new();
    
    let configs = load_all_api_configs();
    let keys = load_api_keys();
    
    println!();
    log_info("🔍 Buscando APIs disponibles...");
    
    let mut apis_found = 0;
    let mut apis_checked = 0;
    
    for config in &configs {
        if !config.enabled {
            continue;
        }
        
        apis_checked += 1;
        
        // Verificar disponibilidad
        if !config.check_availability(&keys, verbose) {
            continue;
        }
        
        apis_found += 1;
        
        // Obtener modelos
        let type_str = if config.is_local() { "local" } else { "cloud" };
        log_info(&format!("📡 {} ({}) - obteniendo modelos...", 
            config.name.cyan(), type_str));
        
        let models = config.fetch_models(&keys);
        
        if models.is_empty() {
            log_debug(&format!("   Sin modelos o error en {}", config.name), verbose);
        } else {
            log_ok(&format!("   {} modelos", models.len()));
            all_models.extend(models);
        }
    }
    
    println!();
    if apis_found == 0 {
        log_warn("⚠️ No se encontraron APIs disponibles");
        log_info("Configura tus APIs:");
        log_info("  - Keys: ~/.config/luismind/keys.env");
        log_info("  - APIs: ~/.config/luismind/apis.conf");
        log_info("  - O inicia un servidor local (ollama, oobabooga, etc.)");
    } else {
        log_ok(&format!("✓ {} APIs activas, {} modelos disponibles", 
            apis_found, all_models.len()));
    }
    
    all_models
}
/// Busca un modelo por nombre (unificado)
fn find_model_unified(query: &str, models: &[DynamicModel]) -> Option<DynamicModel> {
    let query_lower = query.to_lowercase();
    
    // 1. Match exacto por nombre
    if let Some(m) = models.iter().find(|m| m.name.to_lowercase() == query_lower) {
        return Some(m.clone());
    }
    
    // 2. Match exacto por ID
    if let Some(m) = models.iter().find(|m| m.id.to_lowercase() == query_lower) {
        return Some(m.clone());
    }
    
    // 3. Match sin prefijo
    let query_no_prefix = if let Some(pos) = query.find('/') {
        &query[pos + 1..]
    } else {
        query
    };
    let query_no_prefix_lower = query_no_prefix.to_lowercase();
    
    if let Some(m) = models.iter().find(|m| {
        let name_no_prefix = if let Some(pos) = m.name.find('/') {
            &m.name[pos + 1..]
        } else {
            &m.name
        };
        name_no_prefix.to_lowercase() == query_no_prefix_lower
    }) {
        return Some(m.clone());
    }
    
    // 4. Match parcial
    let matches: Vec<&DynamicModel> = models.iter()
        .filter(|m| {
            m.name.to_lowercase().contains(&query_lower) ||
            m.id.to_lowercase().contains(&query_lower) ||
            m.display_name.to_lowercase().contains(&query_lower)
        })
        .collect();
    
    match matches.len() {
        0 => None,
        1 => Some(matches[0].clone()),
        _ => {
            println!();
            log_warn(&format!("'{}' coincide con {} modelos:", query, matches.len()));
            for m in matches.iter().take(10) {
                println!("  {} - {}", m.name.green(), m.display_name);
            }
            if matches.len() > 10 {
                println!("  ... y {} más", matches.len() - 10);
            }
            None
        }
    }
}

/// Obtiene modelos de Ollama
fn get_ollama_models() -> Vec<DynamicModel> {
    let output = Command::new("ollama")
        .args(["list"])
        .output();
    
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut models = Vec::new();
    
    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        
        let raw_name = parts[0];
        let size_info = parts.get(2).map(|s| *s).unwrap_or("");
        
        let user_name = format!("ollama/{}", raw_name);
        let aider_id = format!("ollama_chat/{}", raw_name);
        let context = infer_context_from_model_name(raw_name).max(32768);
        
        let display = if size_info.is_empty() {
            format!("{} (ollama)", raw_name)
        } else {
            format!("{} (ollama) [{}]", raw_name, size_info)
        };
        
        models.push(DynamicModel {
            name: user_name,
            id: aider_id,
            display_name: display,
            provider: "ollama".to_string(),
            token_limit: context,
            is_free: true,
            cooldown_time: 0,  // Local = sin cooldown
        });
    }
    
    models
}
// ═══════════════════════════════════════════════════════════════════════════════
// CLI
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Parser, Debug)]
#[command(name = APP_NAME)]
#[command(about = "Agente de desarrollo autónomo con IA")]
struct Cli {
    /// Directorio del proyecto
    #[arg(default_value = ".")]
    directory: PathBuf,

    /// Modelo a usar (vacío o sin argumento = listar modelos)
    #[arg(short = 'm', long = "model", default_value = "")]
    model: String,

    /// Tarea a realizar en modo autónomo
    #[arg(short = 'a', long = "autonomous")]
    task: Option<String>,

    /// Continuar tarea anterior
    #[arg(short = 'c', long = "continue")]
    continue_mode: bool,

    /// Modo verbose
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Auto-ejecutar/compilar después de cambios
    #[arg(short = 'e', long = "execute")]
    auto_run: bool,

    /// Rotar key al inicio
    #[arg(short = 'r', long = "rotate")]
    force_rotate: bool,

    /// Mezclar modelos si hay errores
    #[arg(long = "mix")]
    mix_models: bool,

    /// Selección dinámica de archivos (máximo N)
    #[arg(short = 'd', long = "dynamic")]
    dynamic_files: Option<usize>,

    /// Timeout por línea en MINUTOS (0 = sin timeout)
    #[arg(short = 't', long = "timeout", default_value = "0")]
    timeout: u64,

    /// Refrescar cache de modelos
    #[arg(long = "refresh")]
    refresh_models: bool,

    /// Limpiar archivos de aider
    #[arg(long = "clean")]
    clean: bool,

    /// Mostrar todo.md
    #[arg(long = "todo")]
    show_todo: bool,

    /// Mostrar estado del sistema
    #[arg(long = "status")]
    status: bool,

    /// Deshacer commits
    #[arg(long = "undo")]
    undo: Option<Option<usize>>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// LISTADO DE MODELOS
// ═══════════════════════════════════════════════════════════════════════════════

fn list_available_models(models: &[DynamicModel]) {
    println!();
    println!("{}", "═══════════════════════════════════════════════════════════════════════════".cyan());
    println!("{}", "                           MODELOS DISPONIBLES                             ".cyan().bold());
    println!("{}", "═══════════════════════════════════════════════════════════════════════════".cyan());
    println!();
    
    if models.is_empty() {
        log_warn("No hay modelos disponibles");
        println!();
        log_info("Verifica tu configuración:");
        log_info("  ~/.config/luismind/keys.env  - API keys");
        log_info("  ~/.config/luismind/apis.conf - Configuración de APIs");
        log_info("  Servidores locales corriendo (ollama, oobabooga, etc.)");
        return;
    }
    
    // Agrupar por proveedor
    let mut by_provider: HashMap<&str, Vec<&DynamicModel>> = HashMap::new();
    for m in models {
        by_provider.entry(m.provider.as_str()).or_default().push(m);
    }
    
    // Cargar configs para saber cuáles son locales
    let configs = load_all_api_configs();
    let local_names: Vec<&str> = configs.iter()
        .filter(|c| c.is_local())
        .map(|c| c.name.as_str())
        .collect();
    
    // Ordenar: locales primero, luego cloud alfabéticamente
    let mut providers: Vec<&str> = by_provider.keys().cloned().collect();
    providers.sort_by(|a, b| {
        let a_local = local_names.contains(a);
        let b_local = local_names.contains(b);
        match (a_local, b_local) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.cmp(b),
        }
    });
    
    for provider in providers {
        if let Some(provider_models) = by_provider.get(provider) {
            let is_local = local_names.contains(&provider);
            print_provider_section(provider, provider_models, is_local);
        }
    }
    
    // Resumen
    println!("{}", "═══════════════════════════════════════════════════════════════════════════".cyan());
    println!("Total: {} modelos de {} proveedores", models.len(), by_provider.len());
    println!();
    
    // Ejemplos de uso
    println!("{}:", "Uso".yellow().bold());
    println!("  {} -m <modelo> -a \"tarea\"   Modo autónomo", APP_NAME);
    println!("  {} -m <modelo>              Modo interactivo", APP_NAME);
    println!("  {} -m <modelo> -c           Continuar tarea", APP_NAME);
    println!();
    
    // Mostrar ejemplo con primer modelo de cada tipo
    if let Some(first) = models.first() {
        println!("{}:", "Ejemplo".yellow().bold());
        println!("  {} -m {} -a \"implementar feature X\"", APP_NAME, first.name);
        println!();
    }
}

fn print_provider_section(provider: &str, models: &[&DynamicModel], is_local: bool) {
    let icon = get_provider_icon(provider);
    let type_label = if is_local { "(local)" } else { "(cloud)" };
    
    // Calcular si todos son free
    let all_free = models.iter().all(|m| m.is_free);
    let free_indicator = if all_free { " 💚" } else { "" };
    
    println!("{} {} {} {} modelos{}", 
        icon,
        provider.to_uppercase().yellow().bold(),
        type_label.dimmed(),
        models.len(),
        free_indicator
    );
    println!("{}", "─".repeat(75));
    
    // Ordenar por nombre
    let mut sorted = models.to_vec();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    
    // Calcular cooldown del provider
    let cooldown_info = if let Some(config) = find_api_config(provider) {
        if config.cooldown_time > 0 {
            format!(" [cooldown: {}s]", config.cooldown_time)
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    
    if !cooldown_info.is_empty() {
        println!("  {}", cooldown_info.dimmed());
    }
    
    for m in sorted.iter().take(20) {
        let ctx_str = format_context(m.token_limit);
        
        println!("  {:<50} {:>8}  {}", 
            m.name.green(),
            ctx_str.cyan(),
            truncate_str(&m.display_name, 18).dimmed()
        );
    }
    
    if models.len() > 20 {
        println!("  {} ... y {} más", "".dimmed(), models.len() - 20);
    }
    
    println!();
}

fn get_provider_icon(provider: &str) -> &'static str {
    match provider.to_lowercase().as_str() {
        "ollama" => "🦙",
        "oobabooga" | "ooba" => "🦎",
        "llama-cpp" | "llama" => "🦙",
        "vllm" => "⚡",
        "local" | "custom" => "🖥️",
        "gemini" => "💎",
        "groq" => "⚡",
        "deepseek" => "🔍",
        "openrouter" => "🌐",
        "together" => "🤝",
        "fireworks" => "🎆",
        "anthropic" | "claude" => "🧠",
        "openai" | "gpt" => "🤖",
        "cohere" => "🔷",
        "mistral" => "🌀",
        "cerebras" => "🧊",
        "sambanova" => "⚡",
        "novita" => "✨",
        "hyperbolic" => "🌊",
        "chutes" => "🪂",
        _ => "🤖",
    }
}

fn format_context(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{}M", tokens / 1_000_000)
    } else if tokens >= 1000 {
        format!("{}k", tokens / 1000)
    } else {
        format!("{}", tokens)
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", chars.iter().take(max_len.saturating_sub(3)).collect::<String>())
    }
}
// ═══════════════════════════════════════════════════════════════════════════════
// MAIN
// ═══════════════════════════════════════════════════════════════════════════════

fn main() {
    setup_ctrlc_handler();
    let cli = Cli::parse();
    let workspace = fs::canonicalize(&cli.directory).unwrap_or(cli.directory.clone());

    // Manejar undo
    if let Some(undo_opt) = &cli.undo {
        match undo_opt {
            Some(num) => { undo_to_commit(&workspace, *num); return; }
            None => { show_git_commits(&workspace); return; }
        }
    }

    if cli.clean { 
        clean_aider_files(&workspace); 
        return; 
    }

    // Obtener todos los modelos
    let all_models = get_all_models_unified(cli.verbose);

    if cli.refresh_models { 
        log_ok(&format!("✓ {} modelos actualizados", all_models.len())); 
        return; 
    }

    // ═══════════════════════════════════════════════════════════════
    // SI NO HAY MODELO O ESTÁ VACÍO = LISTAR MODELOS
    // ═══════════════════════════════════════════════════════════════
    if cli.model.is_empty() {
        list_available_models(&all_models);
        return;
    }

    // ═══════════════════════════════════════════════════════════════
    // RESOLVER MODELO
    // ═══════════════════════════════════════════════════════════════
    let model = match find_model_by_name(&cli.model, &all_models) {
        Some(m) => m,
        None => {
            log_error(&format!("❌ Modelo '{}' no encontrado", cli.model));
            println!();
            log_info("Usa -m sin argumentos para ver modelos disponibles:");
            log_info(&format!("  {} -m", APP_NAME));
            return;
        }
    };

    // Encontrar configuración de API
    let api_config = match find_api_for_model(&model) {
        Some(c) => c,
        None => {
            log_error(&format!("❌ No se encontró configuración para API '{}'", model.provider));
            return;
        }
    };

    // Crear estado
    let model_config = ModelConfig {
        id: model.id.clone(),
        name: model.name.clone(),
        display_name: model.display_name.clone(),
        provider: model.provider.clone(),
        token_limit: model.token_limit,
    };

    let mut state = AppState::new(
        workspace.clone(), 
        model_config, 
        cli.force_rotate, 
        cli.verbose, 
        cli.auto_run, 
        cli.mix_models
    );
    state.load_keys();

    if cli.show_todo { 
        show_todo_file(&state.todo_file); 
        return; 
    }
    if cli.status { 
        show_status(&state, &all_models); 
        return; 
    }

    print_banner();
    
    if !check_requirements(&mut state) { 
        return; 
    }

    // Guardar API config en estado para uso posterior
    // (puedes agregar un campo api_config a AppState si quieres)

    if cli.force_rotate && api_config.api_type == ApiType::Cloud {
        if state.rotate_key() { 
            log_info("✓ Key rotada"); 
        }
    }

    let dynamic_mode = cli.dynamic_files.is_some();
    let max_files = cli.dynamic_files.unwrap_or(20);
    let timeout_minutes = cli.timeout;

    // ═══════════════════════════════════════════════════════════════
    // CARGAR TAREA
    // ═══════════════════════════════════════════════════════════════
    let mut task = String::new();
    
    if cli.continue_mode || state.todo_file.exists() {
        let (steps, handoff) = state.load_todo();
        if let Some(s) = steps { 
            log_todo("📋 Cargando tareas pendientes..."); 
            state.last_next_steps = s.clone(); 
            task = format!("Continue with pending tasks:\n\n{}", s); 
        }
        if let Some(h) = handoff { 
            state.last_agent_handoff = h; 
        }
    }
    
    if let Some(t) = cli.task { 
        task = if task.is_empty() { 
            t 
        } else { 
            format!("{}\n\nAdditional task: {}", task, t) 
        }; 
    }

    // ═══════════════════════════════════════════════════════════════
    // EJECUTAR
    // ═══════════════════════════════════════════════════════════════
    if !task.is_empty() {
        // Modo autónomo
        run_autonomous(&mut state, &task, dynamic_mode, max_files, timeout_minutes);
    } else if cli.continue_mode && !state.last_next_steps.is_empty() {
        // Continuar
        run_autonomous(&mut state, "Continue with pending tasks.", dynamic_mode, max_files, timeout_minutes);
    } else {
        // Modo interactivo
        log_info("Iniciando modo interactivo...");
        run_interactive(&state, &api_config);
    }
}

fn run_aider(
    state: &mut AppState, 
    message: &str, 
    dynamic_mode: bool, 
    max_files: usize,
    timeout_minutes: u64,
) -> AiderResult {
    if should_exit() {
        return AiderResult::Killed;
    }

    // ═══════════════════════════════════════════════════════════════
    // OBTENER CONFIG DE API
    // ═══════════════════════════════════════════════════════════════
    let mut api_config = find_api_config_for_provider(&state.model.provider)
        .unwrap_or_else(|| {
            log_warn(&format!("No se encontró config para {}, usando defaults", state.model.provider));
            ApiConfig {
                name: state.model.provider.clone(),
                prefix: state.model.provider.clone(),
                ..Default::default()
            }
        });

    log_prompt(message, state.verbose);

    // ═══════════════════════════════════════════════════════════════
    // SELECCIONAR ARCHIVOS
    // ═══════════════════════════════════════════════════════════════
    log_info("📂 Seleccionando archivos...");
    let files = if dynamic_mode {
        select_files_dynamic(state, message, max_files)
    } else {
        select_files(&state.workspace, state.model.token_limit)
    };
    
    if files.is_empty() {
        return AiderResult::Error("No se encontraron archivos fuente".into());
    }
    
    log_file(&format!("{} archivos a procesar:", files.len()));
    for f in files.iter().take(5) {
        let rel = f.strip_prefix(&state.workspace).unwrap_or(f);
        println!("    {}", rel.display().to_string().dimmed());
    }
    if files.len() > 5 {
        println!("    ... y {} más", files.len() - 5);
    }

    env::set_var("BROWSER", "/bin/true");
    env::set_var("DISPLAY", "");

    // ═══════════════════════════════════════════════════════════════
    // VERIFICAR API LOCAL
    // ═══════════════════════════════════════════════════════════════
    if api_config.is_local() {
        let keys = load_api_keys();
        if !api_config.is_available(&keys) {
            log_error(&format!("❌ {} no disponible en {}:{}", 
                api_config.name, api_config.host, api_config.port));
            return AiderResult::Error("API local no disponible".into());
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // INFO DE CONEXIÓN
    // ═══════════════════════════════════════════════════════════════
    println!();
    let type_str = if api_config.is_local() { "local" } else { "cloud" };
    log_api(&format!("🌐 Conectando a {} ({})...", api_config.name.to_uppercase(), type_str));
    log_api(&format!("   Modelo: {}", state.model.name.cyan()));
    log_api(&format!("   Contexto: {}", format_context(state.model.token_limit)));
    
    if !api_config.is_local() {
        log_api(&format!("   Cooldown: {}s (max {}s)", 
            api_config.cooldown_time, api_config.max_cooldown));
    }
    
    if timeout_minutes > 0 {
        log_api(&format!("   Timeout: {} min", timeout_minutes));
    }

    // ═══════════════════════════════════════════════════════════════
    // CONSTRUIR COMANDO AIDER
    // ═══════════════════════════════════════════════════════════════
    let mut cmd = Command::new("aider");
    cmd.current_dir(&state.workspace)
        .args([
            "--model", &state.model.id,
            "--dark-mode",
            "--no-auto-commits",
            "--yes-always",
            "--no-suggest-shell-commands",
            "--message", message,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("BROWSER", "/bin/true")
        .env("DISPLAY", "");

    // Configurar aider según el provider
    let keys = load_api_keys();
    api_config.configure_aider(&mut cmd, &keys);
    
    // Mostrar key si es cloud
    if !api_config.is_local() {
        if let Some(key) = api_config.get_api_key(&keys) {
            let masked = mask_key(&key);
            log_key(&format!("🔑 Key: {}", masked));
        }
    }

    for f in &files {
        cmd.arg(f.strip_prefix(&state.workspace).unwrap_or(f));
    }

    log_api("📤 Enviando solicitud...");
    println!();

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log_error(&format!("Error iniciando aider: {}", e));
            return AiderResult::Error(e.to_string());
        }
    };

    log_api("📥 Esperando respuesta...");

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();    // ═══════════════════════════════════════════════════════════════
    // CONFIGURAR LECTURA - CON O SIN TIMEOUT
    // ═══════════════════════════════════════════════════════════════
    let use_timeout = timeout_minutes > 0;
    let timeout_duration = Duration::from_secs(timeout_minutes * 60);
    
    // SIEMPRE crear el receiver (usa el mismo código para ambos casos)
    // La diferencia está en cómo leemos del receiver
    let line_receiver = read_lines_with_timeout(stdout, timeout_duration);
    
    let mut full_output = String::new();
    let mut detected_error: Option<ApiError> = None;
    let mut aider_modified_files: Vec<String> = Vec::new();
    let mut line_count = 0;
    let mut response_started = false;
    let mut last_activity = Instant::now();
    
    // Buffer para SEARCH/REPLACE
    let mut current_file: Option<String> = None;
    let mut in_search = false;
    let mut in_replace = false;
    let mut search_content = String::new();
    let mut replace_content = String::new();

    // ═══════════════════════════════════════════════════════════════
    // LOOP DE LECTURA UNIFICADO
    // ═══════════════════════════════════════════════════════════════
    loop {
        if should_exit() {
            log_warn("Interrupción - guardando estado...");
            state.flush_pending_edits();
            let _ = child.kill();
            return AiderResult::Killed;
        }
        
        // Leer con o sin timeout según configuración
        let recv_result = if use_timeout {
            line_receiver.recv_timeout(timeout_duration)
        } else {
            // Sin timeout real - usar un timeout muy largo (1 hora)
            line_receiver.recv_timeout(Duration::from_secs(3600))
        };
        
        match recv_result {
            Ok(Some(line)) => {
                last_activity = Instant::now();
                
                let result = process_output_line(
                    &line, &mut line_count, &mut response_started,
                    &mut full_output, &mut current_file,
                    &mut in_search, &mut in_replace,
                    &mut search_content, &mut replace_content,
                    &mut aider_modified_files, state
                );
                
                if let Some(error) = result {
                    log_error(&format!("⚡ {}", error.description()));
                    emergency_save_progress(state, &full_output);
                    detected_error = Some(error);
                    let _ = child.kill();
                    break;
                }
                
                // Límite de líneas
                if line_count > MAX_OUTPUT_LINES {
                    log_error(&format!("⚠️ Límite de {} líneas alcanzado", MAX_OUTPUT_LINES));
                    emergency_save_progress(state, &full_output);
                    let _ = child.kill();
                    return AiderResult::Error("Output demasiado largo".into());
                }
            }
            
            Ok(None) => {
                // Fin del stream
                log_debug("Stream finalizado normalmente", state.verbose);
                break;
            }
            
            Err(RecvTimeoutError::Timeout) => {
                if use_timeout {
                    // Solo actuar si el usuario configuró timeout
                    let elapsed_mins = last_activity.elapsed().as_secs() / 60;
                    log_error(&format!("⏰ TIMEOUT: Sin nueva línea por {} min", timeout_minutes));
                    
                    if state.model.provider == "ollama" && !is_ollama_running() {
                        log_error("Ollama no está corriendo");
                    }
                    
                    emergency_save_progress(state, &full_output);
                    let _ = child.kill();
                    return AiderResult::Error(format!(
                        "Timeout: sin respuesta por {} min", timeout_minutes
                    ));
                }
                // Si no hay timeout configurado y llegamos aquí (1 hora), continuar
            }
            
            Err(RecvTimeoutError::Disconnected) => {
                log_debug("Canal desconectado", state.verbose);
                break;
            }
        }
    }

    // Verificar stderr
    let stderr_reader = BufReader::new(stderr);
    for line in stderr_reader.lines().flatten().take(100) {
        let error = detect_api_error(&line);
        if error != ApiError::None && detected_error.is_none() {
            log_debug(&format!("stderr: {}", line), state.verbose);
            detected_error = Some(error);
        }
    }

    if let Some(err) = detected_error {
        state.flush_pending_edits();
        return AiderResult::ApiError(err);
    }

    // Resto del procesamiento (igual que antes)...
    if is_output_too_lazy(&full_output) {
        log_error("❌ RESPUESTA RECHAZADA: Código abreviado");
        let task_preview = truncate_safe(message, 200);
        let redo_note = format!(
            "⚠️ REDO REQUIRED - Response rejected for abbreviated code.\n\
            Task to redo:\n{}\n\n",
            task_preview
        );
        if !state.last_next_steps.contains("REDO REQUIRED") {
            state.last_next_steps = format!("{}{}", redo_note, state.last_next_steps);
        }
        state.save_todo_now();
        return AiderResult::Error("Output rejected: abbreviated code".into());
    }

    let recovered = state.flush_pending_edits();
    if recovered > 0 {
        log_ok(&format!("💾 {} edits recuperados", recovered));
    }

    let (safely_modified, rejected_files) = process_diffs_safely_with_rejected(
        &state.workspace, &full_output, state.verbose
    );
    
    for filename in &rejected_files {
        add_incomplete_file_to_pending(state, filename, "Code rejected");
    }
    
    for f in safely_modified {
        if !aider_modified_files.contains(&f) {
            aider_modified_files.push(f);
        }
    }

    if let Some(steps) = extract_next_steps(&full_output) {
        state.last_next_steps = steps;
    }
    if let Some(handoff) = extract_agent_handoff(&full_output) {
        state.last_agent_handoff = handoff;
    }
    state.save_todo_now();

    match child.wait() {
        Ok(status) => {
            let (sent, recv) = parse_tokens(&full_output);
            state.total_files_modified += aider_modified_files.len();

            println!();
            if !aider_modified_files.is_empty() {
                log_ok(&format!("📝 Modificados: {}", aider_modified_files.join(", ")));
            } else {
                log_warn("⚠️ Sin cambios");
            }

            if status.success() || !aider_modified_files.is_empty() {
                AiderResult::Success {
                    output: full_output,
                    files_modified: aider_modified_files,
                    tokens_sent: sent,
                    tokens_received: recv,
                }
            } else {
                AiderResult::Error(format!("Exit code: {:?}", status.code()))
            }
        }
        Err(e) => AiderResult::Error(e.to_string()),
    }
}


// ═══════════════════════════════════════════════════════════════════════════════
// INTERACTIVE MODE
// ═══════════════════════════════════════════════════════════════════════════════

fn run_interactive(state: &AppState, api_config: &ApiConfig) {
    println!();
    log_info("═══════════════════════════════════════════════════════════════");
    log_info("                    MODO INTERACTIVO                           ");
    log_info("═══════════════════════════════════════════════════════════════");
    println!();
    log_info(&format!("Modelo: {}", state.model.name.cyan()));
    log_info(&format!("Proveedor: {}", api_config.name));
    println!();
    log_info("Iniciando aider en modo interactivo...");
    log_info("Escribe 'exit' o Ctrl+C para salir");
    println!();
    
    let mut cmd = Command::new("aider");
    cmd.current_dir(&state.workspace)
        .args([
            "--model", &state.model.id,
            "--dark-mode",
            "--no-auto-commits",
            "--yes-always",
        ])
        .env("BROWSER", "/bin/true")
        .env("DISPLAY", "");
    
    let keys = load_api_keys();
    api_config.configure_aider_command(&mut cmd, &keys);
    
    // Ejecutar aider interactivamente
    let status = cmd.status();
    
    match status {
        Ok(s) if s.success() => {
            log_ok("Sesión interactiva finalizada");
        }
        Ok(s) => {
            log_warn(&format!("Aider terminó con código: {:?}", s.code()));
        }
        Err(e) => {
            log_error(&format!("Error ejecutando aider: {}", e));
        }
    }
}




/////////////////////////////////////////////
/// 
/// 
/// /////////////////////////////////////////
/// 
/// 

impl LocalApiConfig {
    /// Construye la URL base de la API
    fn get_api_base(&self) -> String {
        if !self.api_base.is_empty() {
            self.api_base.clone()
        } else {
            format!("http://{}:{}/v1", self.host, self.port)
        }
    }
}

/// Carga configuración de APIs locales desde archivo
fn load_local_apis_config() -> Vec<LocalApiConfig> {
    let mut configs = Vec::new();
    
    if let Some(home) = dirs::home_dir() {
        let config_path = home.join(LOCAL_APIS_CONFIG_FILE);
        
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                let mut current_config: Option<LocalApiConfig> = None;
                let mut current_model: Option<LocalModelConfig> = None;
                
                for line in content.lines() {
                    let line = line.trim();
                    
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    
                    // Nueva sección [nombre]
                    if line.starts_with('[') && line.ends_with(']') && !line.starts_with("[[") {
                        // Guardar anterior
                        if let Some(mut cfg) = current_config.take() {
                            if let Some(model) = current_model.take() {
                                cfg.models.push(model);
                            }
                            if cfg.enabled {
                                configs.push(cfg);
                            }
                        }
                        
                        let name = line.trim_matches(|c| c == '[' || c == ']').to_string();
                        current_config = Some(LocalApiConfig {
                            name: name.clone(),
                            provider: name,
                            ..Default::default()
                        });
                        current_model = None;
                    }
                    // Subsección [[model]]
                    else if line.starts_with("[[") && line.ends_with("]]") {
                        if let Some(ref mut cfg) = current_config {
                            if let Some(model) = current_model.take() {
                                cfg.models.push(model);
                            }
                        }
                        current_model = Some(LocalModelConfig {
                            name: "local".to_string(),
                            display_name: "Local Model".to_string(),
                            context: 131072,
                        });
                    }
                    // Key=Value
                    else if let Some(pos) = line.find('=') {
                        let key = line[..pos].trim().to_lowercase();
                        let value = line[pos + 1..].trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string();
                        
                        if let Some(ref mut model) = current_model {
                            match key.as_str() {
                                "name" => model.name = value,
                                "display_name" | "display" => model.display_name = value,
                                "context" | "context_window" | "tokens" => {
                                    model.context = parse_context_value(&value);
                                }
                                _ => {}
                            }
                        } else if let Some(ref mut cfg) = current_config {
                            match key.as_str() {
                                "provider" => cfg.provider = value,
                                "host" => cfg.host = value,
                                "port" => cfg.port = value.parse().unwrap_or(5000),
                                "api_base" | "base_url" | "url" => cfg.api_base = value,
                                "api_key" | "key" => cfg.api_key = value,
                                "context" | "context_window" | "default_context" | "tokens" => {
                                    cfg.default_context = parse_context_value(&value);
                                }
                                "enabled" => cfg.enabled = value == "true" || value == "1" || value == "yes",
                                _ => {}
                            }
                        }
                    }
                }
                
                // Guardar último
                if let Some(mut cfg) = current_config {
                    if let Some(model) = current_model {
                        cfg.models.push(model);
                    }
                    if cfg.enabled {
                        configs.push(cfg);
                    }
                }
            }
        }
    }
    
    // Si no hay configs, crear defaults para APIs comunes
    if configs.is_empty() {
        // Verificar si hay algo corriendo en puertos conocidos
        // Pero NO hardcodeamos - el usuario debe crear el config
    }
    
    configs
}

// ═══════════════════════════════════════════════════════════════════════════════
// CLI - COMMAND LINE INTERFACE
// ═══════════════════════════════════════════════════════════════════════════════
#[derive(Debug, PartialEq)]
enum DiffResult {
    Applied,
    Rejected,
    Skipped,
}

// ═══════════════════════════════════════════════════════════════════════════════
// REQUIREMENTS CHECK
// ═══════════════════════════════════════════════════════════════════════════════

fn check_requirements(state: &mut AppState) -> bool {
    // Verificar aider
    match Command::new("aider").arg("--version").output() {
        Ok(o) if o.status.success() => {
            let version = String::from_utf8_lossy(&o.stdout);
            log_ok(&format!("Aider {}", version.trim()));
        }
        _ => {
            log_error("Aider no encontrado");
            println!("  Instalar: pip install aider-chat");
            return false;
        }
    }

    // Verificar git
    match Command::new("git").arg("--version").output() {
        Ok(o) if o.status.success() => {
            log_debug("Git OK", state.verbose);
        }
        _ => {
            log_warn("Git no encontrado - undo no funcionará");
        }
    }

    // Inicializar git repo si no existe
    ensure_git_repo(&state.workspace);

    // Verificar API key o Ollama
    if state.model.provider != "ollama" {
        if !state.set_api_key() {
            log_error(&format!("No hay {} API key configurada", state.model.provider.to_uppercase()));
            println!("\nConfigurá keys en: ~/.config/rustmind/keys.env");
            println!("Ejemplo:");
            println!("  GEMINI_API_KEY=tu_key_aqui");
            println!("  GEMINI_API_KEY_1=otra_key");
            println!("  GROQ_API_KEY=tu_groq_key");
            return false;
        }
    } else {
        start_ollama();
        if !is_ollama_running() { 
            log_error("Ollama no está corriendo"); 
            println!("  Iniciar: ollama serve");
            return false; 
        }
        log_ok(&format!("Ollama: {}", state.model.name));
    }

    // Info del proyecto
    let files = get_source_files(&state.workspace);
    let total_tokens: usize = files.iter().map(|f| count_tokens(f)).sum();
    log_info(&format!(
        "{} archivos, ~{}k tokens", 
        files.len(), 
        total_tokens / 1000
    ));

    if state.todo_file.exists() { 
        log_todo("📋 todo.md existe - usa -c para continuar"); 
    }

    println!();
    true
}

// ═══════════════════════════════════════════════════════════════════════════════
// STATUS COMMAND
// ═══════════════════════════════════════════════════════════════════════════════

fn format_key_count(count: usize) -> ColoredString {
    if count == 0 {
        "0".red()
    } else if count < 3 {
        format!("{}", count).yellow()
    } else {
        format!("{}", count).green()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// CLEAN COMMAND
// ═══════════════════════════════════════════════════════════════════════════════

fn clean_aider_files(workspace: &Path) {
    log_info("Limpiando archivos de caché de aider...");
    
    let mut count = 0;
    
    fn clean_recursive(dir: &Path, count: &mut usize) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                
                // Saltar directorios especiales
                if name == ".git" || name == "target" || name == "node_modules" {
                    continue;
                }
                
                // Patrones a eliminar
                let should_delete = name.starts_with(".aider")
                    || name.contains(".aider.")
                    || name.ends_with(".orig")
                    || name.ends_with(".aider.chat.history.md")
                    || (name == ".env" && path.parent().map(|p| p.join("Cargo.toml").exists()).unwrap_or(false));
                
                if should_delete {
                    let result = if path.is_file() {
                        fs::remove_file(&path)
                    } else {
                        fs::remove_dir_all(&path)
                    };
                    
                    if result.is_ok() {
                        log_ok(&format!("Eliminado: {}", path.display()));
                        *count += 1;
                    }
                } else if path.is_dir() {
                    clean_recursive(&path, count);
                }
            }
        }
    }
    
    clean_recursive(workspace, &mut count);
    
    log_ok(&format!("✓ {} archivos eliminados", count));
}

// ═══════════════════════════════════════════════════════════════════════════════
// SHOW TODO COMMAND
// ═══════════════════════════════════════════════════════════════════════════════

fn show_todo_file(path: &Path) {
    if !path.exists() {
        log_info("No hay todo.md en este directorio");
        return;
    }
    
    println!();
    println!("{}", "📋 Contenido de todo.md".cyan().bold());
    println!("{}", "═".repeat(80));
    
    match fs::read_to_string(path) {
        Ok(content) => {
            for line in content.lines() {
                // Colorear secciones
                if line.starts_with("NEXT_STEPS:") {
                    println!("{}", line.yellow().bold());
                } else if line.starts_with("AGENT_HANDOFF:") {
                    println!("{}", line.green().bold());
                } else if line.trim().starts_with("1.") || line.trim().starts_with("2.") ||
                          line.trim().starts_with("3.") || line.trim().starts_with("-") {
                    println!("  {}", line.white());
                } else {
                    println!("{}", line.dimmed());
                }
            }
        }
        Err(e) => {
            log_error(&format!("Error leyendo todo.md: {}", e));
        }
    }
    
    println!("{}", "═".repeat(80));
    println!();
    println!("{}", "Uso:".cyan());
    println!("  rustmind -c              # Continuar con tareas pendientes");
    println!("  rustmind -c -a 'extra'   # Continuar + agregar tarea");
    println!();
}

// ═══════════════════════════════════════════════════════════════════════════════
// PROCESS ABBREVIATED DIFFS - INTEGRACIÓN CON RUN_AIDER
// ═══════════════════════════════════════════════════════════════════════════════

/// Procesa diffs abreviados del output de aider
/// Retorna lista de archivos modificados exitosamente
fn process_abbreviated_diffs(workspace: &Path, output: &str) -> Vec<String> {
    let mut modified_files = Vec::new();
    
    if !is_abbreviated_diff(output) {
        return modified_files;
    }
    
    log_warn("⚠️ Detectado diff abreviado ('omitting unchanged')");
    log_info("Aplicando de forma segura...");
    
    // Extraer archivos y sus diffs del output
    let mut file_diffs: HashMap<String, String> = HashMap::new();
    let mut current_file: Option<String> = None;
    let mut current_diff = String::new();
    
    for line in output.lines() {
        let trimmed = line.trim();
        
        // Detectar nombre de archivo
        let is_filename = (trimmed.ends_with(".rs") 
            || trimmed.ends_with(".py") 
            || trimmed.ends_with(".toml")
            || trimmed.ends_with(".js")
            || trimmed.ends_with(".ts")
            || trimmed.ends_with(".go")
            || trimmed.ends_with(".java"))
            && !trimmed.contains(' ')
            && !trimmed.starts_with('+')
            && !trimmed.starts_with('-')
            && !trimmed.starts_with("+++")
            && !trimmed.starts_with("---");
        
        if is_filename {
            // Guardar diff anterior si existe
            if let Some(ref file) = current_file {
                if !current_diff.is_empty() {
                    file_diffs.insert(file.clone(), current_diff.clone());
                }
            }
            current_file = Some(trimmed.to_string());
            current_diff.clear();
            continue;
        }
        
        if current_file.is_some() {
            current_diff.push_str(line);
            current_diff.push('\n');
        }
    }
    
    // Guardar último archivo
    if let Some(ref file) = current_file {
        if !current_diff.is_empty() {
            file_diffs.insert(file.clone(), current_diff);
        }
    }
    
    // Aplicar cada diff de forma segura
    for (filename, diff) in &file_diffs {
        let path = workspace.join(filename);
        if !path.exists() {
            log_warn(&format!("Archivo no existe: {}", filename));
            continue;
        }
        
        if let Ok(original) = fs::read_to_string(&path) {
            let original_lines = original.lines().count();
            
            match apply_abbreviated_diff(&original, diff) {
                Ok(new_content) => {
                    let new_lines = new_content.lines().count();
                    
                    // Verificación de seguridad: rechazar si es muy destructivo
                    if new_lines < original_lines / 2 && original_lines > 50 {
                        log_error(&format!(
                            "❌ RECHAZADO {}: {} → {} líneas (demasiado destructivo)", 
                            filename, original_lines, new_lines
                        ));
                        continue;
                    }
                    
                    // Validar sintaxis para Rust
                    if filename.ends_with(".rs") {
                        let brace_balance: i32 = new_content.chars()
                            .map(|c| match c {
                                '{' => 1,
                                '}' => -1,
                                _ => 0,
                            })
                            .sum();
                        
                        if brace_balance != 0 {
                            log_warn(&format!(
                                "⚠️ Saltando {}: llaves desbalanceadas ({})",
                                filename, brace_balance
                            ));
                            continue;
                        }
                    }
                    
                    // Aplicar cambio
                    if fs::write(&path, &new_content).is_ok() {
                        log_ok(&format!(
                            "✓ Aplicado (abreviado): {} ({} → {} líneas)", 
                            filename, original_lines, new_lines
                        ));
                        if !modified_files.contains(&filename.to_string()) {
                            modified_files.push(filename.clone());
                        }
                    }
                }
                Err(e) => {
                    log_debug(&format!("No se pudo aplicar diff a {}: {}", filename, e), true);
                }
            }
        }
    }
    
    modified_files
}

// ═══════════════════════════════════════════════════════════════════════════════
// DETECT_API_ERROR MEJORADO - DETECTA ERRORES DE OLLAMA
// ═══════════════════════════════════════════════════════════════════════════════

fn detect_api_error(line: &str) -> ApiError {
    let trimmed = line.trim();
    let lower = line.to_lowercase();

    // ═══ ERRORES DE OLLAMA (NUEVOS) ═══
    if lower.contains("connection refused") && lower.contains("11434") {
        return ApiError::Connection;
    }
    
    if lower.contains("ollama") {
        if lower.contains("timeout") || lower.contains("timed out") {
            return ApiError::Timeout;
        }
        if lower.contains("error") && lower.contains("memory") {
            return ApiError::TokenLimit;  // OOM = reducir contexto
        }
        if lower.contains("not running") || lower.contains("connection refused") {
            return ApiError::Connection;
        }
    }
    
    // Errores de memoria/contexto
    if lower.contains("out of memory") || lower.contains("oom") ||
       lower.contains("cuda out of memory") || lower.contains("cublas error") {
        return ApiError::TokenLimit;
    }

    // Permission Denied / Banned (403)
    if lower.contains("permission denied")
        || lower.contains("consumer suspended")
        || lower.contains("consumer_suspended")
        || line.contains("\"code\": 403")
        || line.contains("'code': 403")
        || line.contains("code\":403")
    {
        return ApiError::PermissionDenied;
    }

    // Invalid Key (401)
    if lower.contains("api_key_invalid")
        || lower.contains("invalid api key")
        || (lower.contains("401") && lower.contains("unauthorized"))
    {
        return ApiError::KeyInvalid;
    }

    // LiteLLM Errors
    if trimmed.starts_with("litellm.RateLimitError") || trimmed.contains("RateLimitError") {
        if lower.contains("quota")
            || lower.contains("exceeded")
            || lower.contains("resource_exhausted")
        {
            return ApiError::QuotaExhausted;
        }
        return ApiError::RateLimitTemporary;
    }

    if trimmed.starts_with("litellm.AuthenticationError") {
        return ApiError::KeyInvalid;
    }

    if trimmed.starts_with("litellm.BadRequestError") {
        if lower.contains("permission") || lower.contains("403") || lower.contains("consumer") {
            return ApiError::PermissionDenied;
        }
        if lower.contains("quota") || lower.contains("resource_exhausted") {
            return ApiError::QuotaExhausted;
        }
        return ApiError::BadRequest;
    }

    if trimmed.starts_with("litellm.APIConnectionError") {
        return ApiError::Connection;
    }

    if trimmed.starts_with("litellm.Timeout") {
        return ApiError::Timeout;
    }

    if trimmed.starts_with("litellm.ServiceUnavailableError") {
        return ApiError::ServiceUnavailable;
    }

    // Token Limit
    if trimmed.contains("has hit a token limit") || lower.contains("token limit") {
        return ApiError::TokenLimit;
    }

    // Retry Pattern
    if trimmed.starts_with("Retrying in ") && trimmed.contains("seconds") {
        return ApiError::RateLimitTemporary;
    }

    // Model Overloaded
    if lower.contains("model is overloaded") || lower.contains("overloaded") {
        return ApiError::ModelOverloaded;
    }

    // VertexAI
    if trimmed.contains("VertexAIException") {
        if lower.contains("403") || lower.contains("permission") {
            return ApiError::PermissionDenied;
        }
        if lower.contains("429") || lower.contains("quota") {
            return ApiError::QuotaExhausted;
        }
    }

    ApiError::None
}

// ═══════════════════════════════════════════════════════════════════════════════
// VALIDACIÓN DE SEGURIDAD DE CAMBIOS
// ═══════════════════════════════════════════════════════════════════════════════

/// Valida que un cambio no sea destructivo
/// 
/// 
fn validate_change_safety(original: &str, new_content: &str, filename: &str) -> Result<(), String> {
    let orig_lines = original.lines().count();
    let new_lines = new_content.lines().count();
    
    // Si el archivo nuevo es significativamente más pequeño, rechazar
    if new_lines < orig_lines / 3 && orig_lines > 20 {
        return Err(format!(
            "{}: Cambio destructivo: {} líneas → {} líneas (reducción >66%)",
            filename, orig_lines, new_lines
        ));
    }
    
    // Si el contenido nuevo es mucho más corto, rechazar
    if new_content.len() < original.len() / 2 && original.len() > 500 {
        return Err(format!(
            "{}: {} bytes → {} bytes (reducción >50%)",
            filename, original.len(), new_content.len()
        ));
    }
    
    // Verificar que no esté vacío
    if new_content.trim().is_empty() && !original.trim().is_empty() {
        return Err(format!("{}: El cambio vaciaría el archivo", filename));
    }
    
    // Verificar código lazy en el resultado final
    if contains_lazy_pattern(new_content) {
        return Err(format!("{}: El resultado contiene código lazy/omitido", filename));
    }
    
    // Verificar que no se pierdan funciones importantes
    let orig_fn_count = original.matches("fn ").count();
    let new_fn_count = new_content.matches("fn ").count();
    if new_fn_count < orig_fn_count / 2 && orig_fn_count > 3 {
        return Err(format!(
            "{}: Posible pérdida de funciones: {} → {}",
            filename, orig_fn_count, new_fn_count
        ));
    }
    
    Ok(())
}
/// Encuentra el final de un bloque de código (función, struct, impl, etc.)
fn find_block_end(content: &str, start_pos: usize) -> usize {
    let bytes = content.as_bytes();
    let mut brace_count = 0;
    let mut paren_count = 0;
    let mut in_string = false;
    let mut in_char = false;
    let mut escape_next = false;
    let mut found_first_brace = false;
    
    for i in start_pos..bytes.len() {
        let c = bytes[i] as char;
        
        if escape_next {
            escape_next = false;
            continue;
        }
        
        if c == '\\' {
            escape_next = true;
            continue;
        }
        
        if c == '"' && !in_char {
            in_string = !in_string;
            continue;
        }
        
        if c == '\'' && !in_string {
            in_char = !in_char;
            continue;
        }
        
        if in_string || in_char {
            continue;
        }
        
        match c {
            '{' => {
                brace_count += 1;
                found_first_brace = true;
            }
            '}' => {
                brace_count -= 1;
                if found_first_brace && brace_count == 0 {
                    return i + 1;
                }
            }
            '(' => paren_count += 1,
            ')' => paren_count -= 1,
            _ => {}
        }
    }
    
    // Si no se encontró el final, retornar el final del contenido
    content.len()
}

// ═══════════════════════════════════════════════════════════════════════════════
// SMART DIFF PARSER - SUPPORTS "OMITTING UNCHANGED PARTS" SAFELY
// ═══════════════════════════════════════════════════════════════════════════════

/// Detect if output contains abbreviated diffs
fn is_abbreviated_diff(content: &str) -> bool {
    let patterns = [
        "// ... (omitting",
        "// ...(omitting",
        "(omitting unchanged",
        "// ... unchanged",
        "# ... (rest",
        "// ... rest of",
        "// remaining unchanged",
        "/* ... */",
        "# ...",
        "// ...",
    ];
    let lower = content.to_lowercase();
    patterns
        .iter()
        .any(|p| lower.contains(&p.to_lowercase()))
}

/// Extract function/struct/block name from a code line
fn extract_block_name(line: &str) -> Option<String> {
    let trimmed = line.trim();

    // Skip comments and empty lines
    if trimmed.starts_with("//") || trimmed.starts_with("#") || trimmed.is_empty() {
        return None;
    }

    // Rust: fn name, pub fn name, async fn name, pub async fn name
    if trimmed.contains("fn ") && (trimmed.starts_with("fn ") 
        || trimmed.starts_with("pub fn ")
        || trimmed.starts_with("async fn ")
        || trimmed.starts_with("pub async fn ")
        || trimmed.contains(" fn ")) 
    {
        let parts: Vec<&str> = trimmed.split("fn ").collect();
        if parts.len() > 1 {
            if let Some(name) = parts[1].split('(').next() {
                let clean_name = name.trim().split('<').next().unwrap_or(name.trim());
                if !clean_name.is_empty() {
                    return Some(format!("fn {}", clean_name));
                }
            }
        }
    }

    // Rust: struct Name, pub struct Name
    if trimmed.contains("struct ")
        && (trimmed.starts_with("struct ") || trimmed.starts_with("pub struct "))
    {
        let parts: Vec<&str> = trimmed.split("struct ").collect();
        if parts.len() > 1 {
            if let Some(name) = parts[1]
                .split(|c| c == ' ' || c == '{' || c == '(' || c == '<')
                .next()
            {
                if !name.trim().is_empty() {
                    return Some(format!("struct {}", name.trim()));
                }
            }
        }
    }

    // Rust: impl Name, impl Trait for Name
    if trimmed.starts_with("impl ") {
        let end = trimmed.find('{').unwrap_or(trimmed.len());
        return Some(trimmed[..end].trim().to_string());
    }

    // Rust: enum Name
    if trimmed.contains("enum ")
        && (trimmed.starts_with("enum ") || trimmed.starts_with("pub enum "))
    {
        let parts: Vec<&str> = trimmed.split("enum ").collect();
        if parts.len() > 1 {
            if let Some(name) = parts[1].split(|c| c == ' ' || c == '{' || c == '<').next() {
                if !name.trim().is_empty() {
                    return Some(format!("enum {}", name.trim()));
                }
            }
        }
    }

    // Rust: const/static
    if trimmed.starts_with("const ") || trimmed.starts_with("pub const ") {
        if let Some(name) = trimmed.split(':').next() {
            return Some(name.trim().to_string());
        }
    }

    // Python: def name, class Name
    if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
        if let Some(name) = trimmed.split('(').next() {
            return Some(name.trim().to_string());
        }
    }
    if trimmed.starts_with("class ") {
        if let Some(name) = trimmed.split(|c| c == '(' || c == ':').next() {
            return Some(name.trim().to_string());
        }
    }

    // JavaScript/TypeScript: function name, const name =
    if trimmed.starts_with("function ") || trimmed.starts_with("async function ") {
        if let Some(name) = trimmed.split('(').next() {
            return Some(name.trim().to_string());
        }
    }

    None
}

/// Find the bounds (start, end byte positions) of a code block
fn find_block_bounds(content: &str, block_start_line: usize) -> Option<(usize, usize)> {
    let lines: Vec<&str> = content.lines().collect();
    if block_start_line >= lines.len() {
        return None;
    }

    // Calculate start position in bytes
    let start_pos: usize = lines[..block_start_line]
        .iter()
        .map(|l| l.len() + 1)
        .sum();

    let mut brace_count = 0;
    let mut paren_count = 0;
    let mut started = false;
    let mut end_pos = start_pos;

    for (i, line) in lines[block_start_line..].iter().enumerate() {
        end_pos += line.len() + 1;

        for ch in line.chars() {
            match ch {
                '{' => {
                    brace_count += 1;
                    started = true;
                }
                '}' => {
                    brace_count -= 1;
                }
                '(' => paren_count += 1,
                ')' => paren_count -= 1,
                _ => {}
            }
        }

        // Block is complete when we return to zero braces after starting
        if started && brace_count == 0 {
            return Some((start_pos, end_pos));
        }

        // Safety: don't search more than 1000 lines
        if i > 1000 {
            break;
        }
    }

    // For items without braces (like const declarations)
    if !started {
        // Single line item
        if lines[block_start_line].contains(';') {
            return Some((start_pos, start_pos + lines[block_start_line].len() + 1));
        }
    }

    None
}

/// Replace a block by name in the content
fn replace_block_by_name(content: &str, block_name: &str, new_content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();

    // Search for the line that defines this block
    for (i, line) in lines.iter().enumerate() {
        if let Some(name) = extract_block_name(line) {
            // Check if names match (with some flexibility)
            let name_match = name == block_name
                || block_name.contains(&name)
                || name.contains(block_name)
                || {
                    // Compare just the function/struct name part
                    let n1 = name.split_whitespace().last().unwrap_or(&name);
                    let n2 = block_name.split_whitespace().last().unwrap_or(block_name);
                    n1 == n2
                };

            if name_match {
                // Found! Now find the block bounds
                if let Some((start, end)) = find_block_bounds(content, i) {
                    let before = &content[..start];
                    let after = &content[end..];

                    // Preserve indentation
                    let original_indent = line.len() - line.trim_start().len();
                    let indent_str = " ".repeat(original_indent);

                    // Apply indentation to new content
                    let indented_new: String = new_content
                        .trim()
                        .lines()
                        .enumerate()
                        .map(|(i, l)| {
                            if i == 0 {
                                l.to_string()
                            } else if l.trim().is_empty() {
                                String::new()
                            } else {
                                format!("{}{}", indent_str, l.trim_start())
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");

                    return Some(format!("{}{}\n{}", before, indented_new, after));
                }
            }
        }
    }

    None
}

/// Parse and apply an abbreviated diff safely
/// Returns Ok(new_content) on success, Err(reason) on failure
fn apply_abbreviated_diff(original: &str, diff_content: &str) -> Result<String, String> {
    let mut result = original.to_string();
    let mut replacements_made = 0;
    let mut current_block_name: Option<String> = None;
    let mut current_block_content = String::new();
    let mut in_code_block = false;
    let mut skip_omitting = false;

    for line in diff_content.lines() {
        let trimmed = line.trim();

        // Skip "omitting" marker lines
        if trimmed.to_lowercase().contains("omitting")
            || trimmed.to_lowercase().contains("unchanged")
            || trimmed == "// ..."
            || trimmed == "# ..."
            || trimmed == "/* ... */"
            || (trimmed.starts_with("//") && trimmed.contains("..."))
        {
            skip_omitting = true;
            continue;
        }

        // Detect new block start
        if let Some(name) = extract_block_name(trimmed) {
            // If we had a previous block, try to apply it
            if let Some(ref block_name) = current_block_name {
                if !current_block_content.trim().is_empty() {
                    if let Some(new_result) =
                        replace_block_by_name(&result, block_name, &current_block_content)
                    {
                        result = new_result;
                        replacements_made += 1;
                    }
                }
            }

            current_block_name = Some(name);
            current_block_content.clear();
            in_code_block = true;
            skip_omitting = false;

            // Add this first line
            let clean_line = line
                .trim_start_matches('+')
                .trim_start_matches('-')
                .trim_start_matches(' ');
            current_block_content.push_str(clean_line);
            current_block_content.push('\n');
        } else if in_code_block && !skip_omitting {
            // Accumulate block content
            let clean_line = line
                .trim_start_matches('+')
                .trim_start_matches('-');

            // Don't add diff markers or empty diff lines
            if !clean_line.starts_with("@@")
                && !clean_line.starts_with("---")
                && !clean_line.starts_with("+++")
            {
                current_block_content.push_str(clean_line);
                current_block_content.push('\n');
            }
        }
    }

    // Apply last block
    if let Some(ref block_name) = current_block_name {
        if !current_block_content.trim().is_empty() {
            if let Some(new_result) =
                replace_block_by_name(&result, block_name, &current_block_content)
            {
                result = new_result;
                replacements_made += 1;
            }
        }
    }

    if replacements_made > 0 {
        Ok(result)
    } else {
        Err("Could not apply any changes from abbreviated diff".to_string())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MODELS - DYNAMIC DISCOVERY AND MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
struct ModelVersion {
    id: String,          // Full ID for aider (e.g., "gemini/gemini-2.5-flash")
    name: String,        // Short name for CLI (e.g., "gemini-2.5-flash")
    display_name: String,
    provider: String,
    token_limit: usize,
    is_free: bool,
}
#[derive(Debug, Clone)]
struct ModelConfig {
    /// ID para aider (ej: "gemini/gemini-2.5-pro", "ollama_chat/qwen-coder")
    id: String,
    /// Nombre para mostrar/buscar
    name: String,
    /// Nombre legible
    display_name: String,
    /// Provider
    provider: String,
    /// Límite de tokens
    token_limit: usize,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            display_name: String::new(),
            provider: String::new(),
            token_limit: 32768,
        }
    }
}

impl From<DynamicModel> for ModelConfig {
    fn from(m: DynamicModel) -> Self {
        Self {
            id: m.id,
            name: m.name,
            display_name: m.display_name,
            provider: m.provider,
            token_limit: m.token_limit,
        }
    }
}

impl From<&DynamicModel> for ModelConfig {
    fn from(m: &DynamicModel) -> Self {
        Self {
            id: m.id.clone(),
            name: m.name.clone(),
            display_name: m.display_name.clone(),
            provider: m.provider.clone(),
            token_limit: m.token_limit,
        }
    }
}

fn get_models_cache_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/rustmind/models_cache.txt")
}

fn get_banned_keys_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/rustmind/banned_keys.txt")
}

fn save_models_cache(models: &[DynamicModel]) {
    let cache_path = get_models_cache_path();
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let content: String = models
        .iter()
        .map(|m| {
            format!(
                "{}|{}|{}|{}|{}|{}",
                m.id, m.name, m.display_name, m.provider, m.token_limit, m.is_free
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&cache_path, content).ok();
}

fn load_models_cache() -> Vec<DynamicModel> {
    let cache_path = get_models_cache_path();
    if let Ok(content) = fs::read_to_string(&cache_path) {
        return content
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 6 {
                    Some(DynamicModel {
                        name: parts[1].to_string(),
                        display_name: parts[2].to_string(),
                        provider: parts[3].to_string(),
                        token_limit: parts[4].parse().unwrap_or(100_000),
                        is_free: parts[5] == "true",

                    })
                } else {
                    None
                }
            })
            .collect();
    }
    Vec::new()
}

fn load_banned_keys() -> HashSet<String> {
    let path = get_banned_keys_path();
    if let Ok(content) = fs::read_to_string(&path) {
        return content.lines().map(|s| s.to_string()).collect();
    }
    HashSet::new()
}

fn save_banned_key(key: &str) {
    let path = get_banned_keys_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }

    let mut keys = load_banned_keys();
    let masked = mask_key(&key);

    if keys.insert(key.to_string()) {
        let content: String = keys.into_iter().collect::<Vec<_>>().join("\n");
        if fs::write(&path, content).is_ok() {
            log_error(&format!(
                "🚫 Key {} added to permanent ban list",
                masked
            ));
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MODEL PROVIDERS - FETCH AND PARSE
// ═══════════════════════════════════════════════════════════════════════════════


fn parse_openrouter_models(json: &str) -> Vec<DynamicModel> {
    let mut models = Vec::new();
    let mut current_id = String::new();
    let mut is_free = false;

    for line in json.lines() {
        let line = line.trim();
        if line.contains("\"id\":") {
            if let Some(start) = line.find(": \"") {
                if let Some(end) = line[start + 3..].find('"') {
                    current_id = line[start + 3..start + 3 + end].to_string();
                }
            }
        }
        if line.contains("\":free\"") || line.contains("\": \"0\"") {
            is_free = true;
        }
        if line.contains("}") && !current_id.is_empty() {
            if (current_id.contains("claude")
                || current_id.contains("gemini")
                || current_id.contains("gpt")
                || current_id.contains("deepseek")
                || current_id.contains("llama")
                || current_id.contains("qwen"))
                && !current_id.contains("vision")
                && !current_id.contains("image")
            {
                let display = current_id
                    .split('/')
                    .last()
                    .unwrap_or(&current_id)
                    .to_string();
                let short_name = display.clone();

                models.push(DynamicModel {
                    name: short_name,
                    display_name: display,
                    provider: "openrouter".to_string(),
                    token_limit: 128_000,
                    is_free,
                });
            }
            current_id.clear();
            is_free = false;
        }
    }

    models.truncate(50);
    models
}

// ═══════════════════════════════════════════════════════════════════════════════
// PARSING - EXTRACT EDITS AND INFORMATION FROM OUTPUT
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct PendingEdit {
    filename: String,
    search: String,
    replace: String,
}

fn extract_pending_edits(output: &str) -> Vec<PendingEdit> {
    let mut edits = Vec::new();
    edits.extend(extract_search_replace_edits(output));
    edits.extend(extract_unified_diff_edits(output));
    edits
}

fn apply_pending_edits(workspace: &Path, edits: &[PendingEdit]) -> usize {
    let mut applied = 0;

    for edit in edits {
        let path = workspace.join(&edit.filename);
        if !path.exists() {
            continue;
        }

        if let Ok(content) = fs::read_to_string(&path) {
            let search = edit.search.trim();
            if content.contains(search) {
                let new = content.replace(search, edit.replace.trim());
                if new != content && fs::write(&path, &new).is_ok() {
                    log_ok(&format!("💾 Recovered: {}", edit.filename));
                    applied += 1;
                }
            }
        }
    }

    applied
}

fn has_applied_edits(output: &str) -> bool {
    output.contains("Applied edit to")
}

fn get_modified_files(output: &str) -> Vec<String> {
    let mut files = Vec::new();

    for line in output.lines() {
        if line.contains("Applied edit to") {
            if let Some(f) = line.split("Applied edit to").nth(1) {
                let file = f.trim().to_string();
                if !files.contains(&file) && !file.is_empty() {
                    files.push(file);
                }
            }
        }

        let trimmed = line.trim();
        if trimmed.starts_with("+++ b/") {
            let path = trimmed.trim_start_matches("+++ b/").trim();
            if !path.is_empty()
                && !path.starts_with("/dev/null")
                && !files.contains(&path.to_string())
            {
                files.push(path.to_string());
            }
        }
    }

    files
}

fn parse_tokens(output: &str) -> (usize, usize) {
    for line in output.lines() {
        if line.contains("Tokens:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let mut sent = 0;
            let mut recv = 0;

            for (i, p) in parts.iter().enumerate() {
                if *p == "sent," && i > 0 {
                    sent = parts[i - 1]
                        .replace("k", "")
                        .replace(".", "")
                        .parse()
                        .unwrap_or(0)
                        * 1000;
                }
                if *p == "received." && i > 0 {
                    recv = parts[i - 1]
                        .replace("k", "")
                        .replace(".", "")
                        .parse()
                        .unwrap_or(0)
                        * 1000;
                }
            }

            return (sent, recv);
        }
    }
    (0, 0)
}

fn extract_next_steps(output: &str) -> Option<String> {
    if let Some(pos) = output.rfind("NEXT_STEPS:") {
        let after = &output[pos + 11..];
        let end = after
            .find("AGENT_HANDOFF:")
            .or_else(|| after.find("Tokens:"))
            .or_else(|| after.find("TASK COMPLETED"))
            .unwrap_or(after.len().min(2000));

        let steps = after[..end].trim();
        if steps.len() > 5 {
            let lower = steps.to_lowercase();
            if lower.contains("none") && lower.len() < 50 {
                return None;
            }
            return Some(steps.to_string());
        }
    }
    None
}

fn extract_agent_handoff(output: &str) -> Option<String> {
    if let Some(pos) = output.rfind("AGENT_HANDOFF:") {
        let after = &output[pos + 14..];
        let end = after
            .find("Tokens:")
            .or_else(|| after.find("TASK COMPLETED"))
            .unwrap_or(after.len().min(2000));

        let handoff = after[..end].trim();
        if handoff.len() > 20 {
            return Some(handoff.to_string());
        }
    }
    None
}

fn is_truly_complete(output: &str, next_steps: &Option<String>) -> bool {
    let lower = output.to_lowercase();
    if !lower.contains("task completed") {
        return false;
    }

    if let Some(steps) = next_steps {
        let steps_lower = steps.to_lowercase();
        if steps_lower.contains("none") || steps_lower.contains("all complete") {
            return true;
        }

        let real_items = steps
            .lines()
            .filter(|l| {
                let t = l.trim();
                (t.starts_with("1.")
                    || t.starts_with("2.")
                    || t.starts_with("-")
                    || t.starts_with("["))
                    && t.len() > 10
            })
            .count();

        if real_items > 0 {
            return false;
        }
    }

    true
}

// ═══════════════════════════════════════════════════════════════════════════════
// COMPILATION
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
enum CompileResult {
    Success,
    Failed { errors: String },
    NotProject,
}

fn try_compile(workspace: &Path, verbose: bool) -> CompileResult {
    if !workspace.join("Cargo.toml").exists() {
        return CompileResult::NotProject;
    }

    log_info("🔨 Compiling...");
    let output = Command::new("cargo")
        .args(["build"])
        .current_dir(workspace)
        .output();

    match output {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            if verbose {
                for l in stderr.lines().take(20) {
                    println!("{}", l);
                }
            }

            if o.status.success() {
                log_ok("✅ Compilation successful");
                CompileResult::Success
            } else {
                log_error("❌ Compilation errors");
                let errors: String = stderr
                    .lines()
                    .filter(|l| {
                        l.contains("error[E") || l.contains("error:") || l.contains("-->")
                    })
                    .take(40)
                    .collect::<Vec<_>>()
                    .join("\n");

                CompileResult::Failed {
                    errors: if errors.is_empty() { stderr } else { errors },
                }
            }
        }
        Err(e) => CompileResult::Failed {
            errors: e.to_string(),
        },
    }
}

fn try_run(workspace: &Path, verbose: bool) -> Result<String, String> {
    if !workspace.join("Cargo.toml").exists() {
        return Err("No Cargo.toml".into());
    }

    log_info("🚀 Running...");
    let output = Command::new("cargo")
        .args(["run"])
        .current_dir(workspace)
        .output();

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            if verbose {
                println!("{}", stdout);
            }

            if o.status.success() {
                log_ok("✅ Run successful");
                Ok(stdout)
            } else {
                Err(format!(
                    "{}\n{}",
                    stdout,
                    String::from_utf8_lossy(&o.stderr)
                ))
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// APPLICATION STATE
// ═══════════════════════════════════════════════════════════════════════════════
struct AppState {
    workspace: PathBuf,
    todo_file: PathBuf,
    model: ModelConfig,
    original_model_id: String,
    verbose: bool,
    auto_run: bool,
    mix_models: bool,

    // API Keys por proveedor
    gemini_keys: Vec<String>,
    groq_keys: Vec<String>,
    deepseek_keys: Vec<String>,
    openrouter_keys: Vec<String>,
    chutes_keys: Vec<String>,
    sambanova_keys: Vec<String>,
    cerebras_keys: Vec<String>,
    together_keys: Vec<String>,
    fireworks_keys: Vec<String>,
    cohere_keys: Vec<String>,
    huggingface_keys: Vec<String>,
    mistral_keys: Vec<String>,
    novita_keys: Vec<String>,
    hyperbolic_keys: Vec<String>,
    
    current_key_index: usize,
    suspended_keys: HashSet<String>,
    banned_keys: HashSet<String>,

    session_started: bool,
    iteration: usize,
    force_rotate: bool,

    total_files_modified: usize,
    total_tokens_sent: usize,
    total_tokens_received: usize,
    last_key_used: String,

    last_next_steps: String,
    last_agent_handoff: String,
    last_compile_errors: String,

    consecutive_errors: usize,
    model_rotation_index: usize,
    
    // NUEVO: Buffer de edits pendientes para recuperación
    pending_edits_buffer: Vec<PendingEdit>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// API ERROR DETECTION - PRECISE AND CATEGORIZED
// ═══════════════════════════════════════════════════════════════════════════════
#[derive(Debug, Clone, PartialEq)]
enum ApiError {
    None,
    RateLimitTemporary,
    QuotaExhausted,
    PermissionDenied,
    KeyInvalid,
    TokenLimit,
    ModelOverloaded,
    Connection,
    Timeout,
    ServiceUnavailable,
    BadRequest,
}

impl ApiError {
    fn description(&self) -> &str {
        match self {
            ApiError::None => "Sin error",
            ApiError::RateLimitTemporary => "Rate limit temporal - esperar",
            ApiError::QuotaExhausted => "Cuota agotada - rotar key",
            ApiError::PermissionDenied => "Permiso denegado - key baneada",
            ApiError::KeyInvalid => "API key inválida",
            ApiError::TokenLimit => "Límite de tokens alcanzado",
            ApiError::ModelOverloaded => "Modelo sobrecargado",
            ApiError::Connection => "Error de conexión",
            ApiError::Timeout => "Timeout",
            ApiError::ServiceUnavailable => "Servicio no disponible",
            ApiError::BadRequest => "Bad request",
        }
    }
    
    fn wait_seconds(&self) -> u64 {
        match self {
            ApiError::RateLimitTemporary => 30,
            ApiError::QuotaExhausted => 5,
            ApiError::ModelOverloaded => 20,
            ApiError::Connection | ApiError::Timeout => 10,
            ApiError::ServiceUnavailable => 30,
            _ => 2,
        }
    }
    
    fn should_rotate_key(&self) -> bool {
        matches!(self, 
            ApiError::QuotaExhausted | 
            ApiError::PermissionDenied | 
            ApiError::KeyInvalid |
            ApiError::RateLimitTemporary
        )
    }
    
    fn is_permanent_ban(&self) -> bool {
        matches!(self, ApiError::PermissionDenied | ApiError::KeyInvalid)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// APPSTATE - IMPLEMENTACIÓN COMPLETA
// ═══════════════════════════════════════════════════════════════════════════════

impl AppState {
    fn new(
        workspace: PathBuf,
        model: ModelConfig,
        force_rotate: bool,
        verbose: bool,
        auto_run: bool,
        mix_models: bool,
    ) -> Self {
        let config_dir = dirs::home_dir()
            .unwrap_or_default()
            .join(".config/luiggi");
        fs::create_dir_all(config_dir.join("logs")).ok();

        let original_id = model.id.clone();

        Self {
            todo_file: workspace.join("todo.md"),
            workspace,
            model,
            original_model_id: original_id,
            verbose,
            auto_run,
            mix_models,

            gemini_keys: Vec::new(),
            groq_keys: Vec::new(),
            deepseek_keys: Vec::new(),
            openrouter_keys: Vec::new(),
            chutes_keys: Vec::new(),
            sambanova_keys: Vec::new(),
            cerebras_keys: Vec::new(),
            together_keys: Vec::new(),
            fireworks_keys: Vec::new(),
            cohere_keys: Vec::new(),
            huggingface_keys: Vec::new(),
            mistral_keys: Vec::new(),
            novita_keys: Vec::new(),
            hyperbolic_keys: Vec::new(),
            
            current_key_index: 0,
            suspended_keys: HashSet::new(),
            banned_keys: load_banned_keys(),

            session_started: false,
            iteration: 0,
            force_rotate,

            total_files_modified: 0,
            total_tokens_sent: 0,
            total_tokens_received: 0,
            last_key_used: String::new(),

            last_next_steps: String::new(),
            last_agent_handoff: String::new(),
            last_compile_errors: String::new(),

            consecutive_errors: 0,
            model_rotation_index: 0,
            
            pending_edits_buffer: Vec::new(),
        }
    }

    fn load_keys(&mut self) {
        // Intentar nuevo path primero
        let keys_file = dirs::home_dir()
            .unwrap_or_default()
            .join(".config/luiggi/keys.env");
        
        // Fallback al path viejo
        let keys_file = if keys_file.exists() { 
            keys_file 
        } else {
            dirs::home_dir().unwrap_or_default().join(".config/rustmind/keys.env")
        };

        let mut env_vars: HashMap<String, String> = HashMap::new();

        if let Ok(content) = fs::read_to_string(&keys_file) {
            for line in content.lines() {
                let line = line.trim();
                if !line.starts_with('#') && !line.is_empty() {
                    if let Some((k, v)) = line.split_once('=') {
                        env_vars.insert(
                            k.trim().to_string(),
                            v.trim().trim_matches('"').trim_matches('\'').to_string(),
                        );
                    }
                }
            }
        }

        fn load_for(
            vars: &HashMap<String, String>,
            prefix: &str,
            max: usize,
            banned: &HashSet<String>,
        ) -> Vec<String> {
            let mut keys = Vec::new();

            for key in [vars.get(prefix), env::var(prefix).ok().as_ref()]
                .into_iter()
                .flatten()
            {
                if key.len() > 20 && !keys.contains(key) && !banned.contains(key) {
                    keys.push(key.clone());
                }
            }

            for i in 1..=max {
                let name = format!("{}_{}", prefix, i);
                for key in [vars.get(&name), env::var(&name).ok().as_ref()]
                    .into_iter()
                    .flatten()
                {
                    if key.len() > 20 && !keys.contains(key) && !banned.contains(key) {
                        keys.push(key.clone());
                    }
                }
            }

            keys
        }

        self.gemini_keys = load_for(&env_vars, "GEMINI_API_KEY", 20, &self.banned_keys);
        self.groq_keys = load_for(&env_vars, "GROQ_API_KEY", 10, &self.banned_keys);
        self.deepseek_keys = load_for(&env_vars, "DEEPSEEK_API_KEY", 10, &self.banned_keys);
        self.openrouter_keys = load_for(&env_vars, "OPENROUTER_API_KEY", 10, &self.banned_keys);
        self.chutes_keys = load_for(&env_vars, "CHUTES_API_KEY", 10, &self.banned_keys);
        self.sambanova_keys = load_for(&env_vars, "SAMBANOVA_API_KEY", 10, &self.banned_keys);
        self.cerebras_keys = load_for(&env_vars, "CEREBRAS_API_KEY", 10, &self.banned_keys);
        self.together_keys = load_for(&env_vars, "TOGETHER_API_KEY", 10, &self.banned_keys);
        self.fireworks_keys = load_for(&env_vars, "FIREWORKS_API_KEY", 10, &self.banned_keys);
        self.cohere_keys = load_for(&env_vars, "COHERE_API_KEY", 10, &self.banned_keys);
        self.mistral_keys = load_for(&env_vars, "MISTRAL_API_KEY", 10, &self.banned_keys);
        self.novita_keys = load_for(&env_vars, "NOVITA_API_KEY", 10, &self.banned_keys);
        self.hyperbolic_keys = load_for(&env_vars, "HYPERBOLIC_API_KEY", 10, &self.banned_keys);
        
        // HuggingFace tiene dos nombres posibles
        let mut hf_keys = load_for(&env_vars, "HUGGINGFACE_API_KEY", 10, &self.banned_keys);
        hf_keys.extend(load_for(&env_vars, "HF_TOKEN", 10, &self.banned_keys));
        self.huggingface_keys = hf_keys;

        if !self.banned_keys.is_empty() {
            log_warn(&format!(
                "⚠️ {} permanently banned keys (see ~/.config/luiggi/banned_keys.txt)",
                self.banned_keys.len()
            ));
        }
    }

    fn get_keys(&self) -> &Vec<String> {
        match self.model.provider.as_str() {
            "gemini" => &self.gemini_keys,
            "groq" => &self.groq_keys,
            "deepseek" => &self.deepseek_keys,
            "openrouter" => &self.openrouter_keys,
            "chutes" => &self.chutes_keys,
            "sambanova" => &self.sambanova_keys,
            "cerebras" => &self.cerebras_keys,
            "together" => &self.together_keys,
            "fireworks" => &self.fireworks_keys,
            "cohere" => &self.cohere_keys,
            "huggingface" => &self.huggingface_keys,
            "mistral" => &self.mistral_keys,
            "novita" => &self.novita_keys,
            "hyperbolic" => &self.hyperbolic_keys,
            _ => &self.gemini_keys,
        }
    }

    fn keys_count(&self) -> usize {
        if self.model.provider == "ollama" {
            1
        } else {
            self.get_keys().len()
        }
    }

    fn current_key(&self) -> Option<String> {
        let keys = self.get_keys();
        if keys.is_empty() {
            None
        } else {
            keys.get(self.current_key_index % keys.len()).cloned()
        }
    }

    fn set_api_key(&mut self) -> bool {
        if self.model.provider == "ollama" {
            // Validate Ollama model
            if let Err(e) = validate_ollama_model(&self.model.name) {
                log_error(&e);
                return false;
            }
            
            // Set environment variables for optimal performance
            for (key, value) in get_ollama_env_vars_optimized(&self.model.name) {
                env::set_var(&key, &value);
                log_debug(&format!("Set {}={}", key, value), self.verbose);
            }
            
            log_key(&format!("🦙 Ollama: {} ({}k context)", 
                self.model.name, 
                self.model.token_limit / 1000
            ));
            
            return true;
        }
        
        let count = self.keys_count();
        if count == 0 { 
            log_error(&format!("❌ No hay keys para {}", self.model.provider.to_uppercase())); 
            return false; 
        }
        
        let mut attempts = 0;
        while attempts < count {
            if let Some(key) = self.current_key() {
                if !self.suspended_keys.contains(&key) && !self.banned_keys.contains(&key) {
                    let var = match self.model.provider.as_str() {
                        "gemini" => "GEMINI_API_KEY",
                        "groq" => "GROQ_API_KEY",
                        "deepseek" => "DEEPSEEK_API_KEY",
                        "openrouter" => "OPENROUTER_API_KEY",
                        "chutes" => "CHUTES_API_KEY",
                        "sambanova" => "SAMBANOVA_API_KEY",
                        "cerebras" => "CEREBRAS_API_KEY",
                        "together" => "TOGETHER_API_KEY",
                        "fireworks" => "FIREWORKS_API_KEY",
                        "cohere" => "COHERE_API_KEY",
                        "huggingface" => "HUGGINGFACE_API_KEY",
                        "mistral" => "MISTRAL_API_KEY",
                        "novita" => "NOVITA_API_KEY",
                        "hyperbolic" => "HYPERBOLIC_API_KEY",
                        _ => return false,
                    };
                    env::set_var(var, &key);
                    
                    let masked = mask_key(&key);

                    log_key(&format!("🔑 {} Key #{}/{}: {}", 
                        self.model.provider.to_uppercase(), 
                        self.current_key_index + 1, 
                        count, 
                        masked
                    ));
                    
                    if !self.last_key_used.is_empty() && self.last_key_used != key { 
                        log_warn("⚠️ KEY CHANGED"); 
                    }
                    self.last_key_used = key;
                    return true;
                }
            }
            self.current_key_index = (self.current_key_index + 1) % count;
            attempts += 1;
        }
        
        log_error(&format!("❌ Todas las {} keys suspendidas/baneadas", count));
        false
    }

// ═══════════════════════════════════════════════════════════════════════════════
// CASO 1: rotate_key - old es usize, no string
// ═══════════════════════════════════════════════════════════════════════════════

    fn rotate_key(&mut self) -> bool {
        let count = self.keys_count();
        if count <= 1 {
            log_warn("Only 1 key available");
            return false;
        }

        let old = self.current_key_index;
        let old_masked = self.current_key()
            .map(|k| mask_key(&k))
            .unwrap_or_else(|| "???".to_string());

        let mut attempts = 0;
        loop {
            self.current_key_index = (self.current_key_index + 1) % count;
            attempts += 1;

            if let Some(key) = self.current_key() {
                if !self.suspended_keys.contains(&key) && !self.banned_keys.contains(&key) {
                    log_key(&format!(
                        "🔄 Rotating: Key #{} ({}) → #{}",
                        old + 1,
                        old_masked,
                        self.current_key_index + 1
                    ));
                    return self.set_api_key();
                }
            }

            if attempts >= count {
                log_error("❌ No valid keys available");
                return false;
            }
        }
    }

    fn mark_suspended(&mut self, permanent: bool) {
        if let Some(key) = self.current_key() {
            let masked = mask_key(&key);

            if permanent {
                log_error(&format!(
                    "🚫 Key #{} PERMANENTLY BANNED: {}",
                    self.current_key_index + 1,
                    masked
                ));
                self.banned_keys.insert(key.clone());
                save_banned_key(&key);
            } else {
                log_warn(&format!(
                    "⚠️ Key #{} temporarily suspended: {}",
                    self.current_key_index + 1,
                    masked
                ));
                self.suspended_keys.insert(key);
            }

            log_info(&format!(
                "Keys: {} active, {} suspended, {} banned",
                self.keys_count()
                    .saturating_sub(self.suspended_keys.len())
                    .saturating_sub(self.banned_keys.len()),
                self.suspended_keys.len(),
                self.banned_keys.len()
            ));
        }
    }

    fn try_alternative_model(&mut self, all_models: &[DynamicModel]) -> bool {
        if !self.mix_models {
            return false;
        }

        let alternatives = get_alternative_models(&self.model.provider, &self.model.id, all_models);
        if alternatives.is_empty() {
            return false;
        }

        let next_idx = self.model_rotation_index % alternatives.len();
        let alt_model_id = &alternatives[next_idx];
        self.model_rotation_index += 1;

        if let Some(alt) = all_models.iter().find(|m| &m.id == alt_model_id) {
            log_info(&format!(
                "🔀 Probando modelo alternativo: {} → {}",
                self.model.name, alt.name
            ));
            self.model.id = alt.id.clone();
            self.model.name = alt.name.clone();
            self.model.display_name = alt.display_name.clone();
            self.model.token_limit = alt.token_limit;
            return true;
        }

        false
    }

    fn restore_original_model(&mut self) {
        if self.model.id != self.original_model_id {
            log_info(&format!(
                "↩️ Restoring original model: {}",
                self.original_model_id
            ));
            self.model.id = self.original_model_id.clone();
            self.model.name = self
                .original_model_id
                .split('/')
                .last()
                .unwrap_or(&self.original_model_id)
                .to_string();
        }
    }

    fn reset_errors(&mut self) {
        self.consecutive_errors = 0;
    }

    fn record_error(&mut self) {
        self.consecutive_errors += 1;
    }

    fn save_todo_now(&mut self) {
        if self.last_next_steps.is_empty() && self.last_agent_handoff.is_empty() {
            return;
        }

        let mut content = String::new();
        
        // Header con timestamp
        //content.push_str(&format!("# Luiggi Code - Estado guardado: {}\n\n", 
        //    chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
        
        if !self.last_next_steps.is_empty() {
            content.push_str("NEXT_STEPS:\n");
            content.push_str(&self.last_next_steps);
            content.push_str("\n\n");
        }
        if !self.last_agent_handoff.is_empty() {
            content.push_str("AGENT_HANDOFF:\n");
            content.push_str(&self.last_agent_handoff);
            content.push('\n');
        }

        // Escribir con sync para garantizar persistencia
        let temp = self.todo_file.with_extension("md.tmp");
        if let Ok(mut f) = std::fs::File::create(&temp) {
            use std::io::Write;
            if f.write_all(content.as_bytes()).is_ok() && f.sync_all().is_ok() {
                if fs::rename(&temp, &self.todo_file).is_ok() {
                    log_todo(&format!("💾 Guardado todo.md ({} bytes)", content.len()));
                    return;
                }
            }
        }

        // Fallback directo
        if let Ok(mut f) = std::fs::File::create(&self.todo_file) {
            use std::io::Write;
            let _ = f.write_all(content.as_bytes());
            let _ = f.sync_all();
            log_todo("💾 Guardado todo.md (fallback)");
        }
    }

    fn load_todo(&self) -> (Option<String>, Option<String>) {
        if let Ok(content) = fs::read_to_string(&self.todo_file) {
            let mut steps = None;
            let mut handoff = None;

            if let Some(pos) = content.find("NEXT_STEPS:") {
                let after = &content[pos + 11..];
                let end = after.find("AGENT_HANDOFF:").unwrap_or(after.len());
                let s = after[..end].trim();
                if !s.is_empty() {
                    steps = Some(s.to_string());
                }
            }

            if let Some(pos) = content.find("AGENT_HANDOFF:") {
                let h = content[pos + 14..].trim();
                if !h.is_empty() {
                    handoff = Some(h.to_string());
                }
            }

            return (steps, handoff);
        }
        (None, None)
    }
    
    /// Guarda edits pendientes para recuperación ante fallos
    fn buffer_pending_edit(&mut self, edit: PendingEdit) {
        self.pending_edits_buffer.push(edit);
        // Auto-flush cada 5 edits
        if self.pending_edits_buffer.len() >= 5 {
            self.flush_pending_edits();
        }
    }
    
    /// Aplica edits pendientes del buffer
    fn flush_pending_edits(&mut self) -> usize {
        if self.pending_edits_buffer.is_empty() {
            return 0;
        }
        
        let count = apply_pending_edits_with_backup(&self.workspace, &self.pending_edits_buffer);
        if count > 0 {
            log_ok(&format!("💾 Recovered {} pending edits from buffer", count));
        }
        self.pending_edits_buffer.clear();
        count
    }
}
// ═══════════════════════════════════════════════════════════════════════════════
// LOGGING - COLORED OUTPUT
// ═══════════════════════════════════════════════════════════════════════════════

fn ts() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

fn log_ok(m: &str) {
    println!("{} {} {}", format!("[{}]", ts()).green(), "✓".green(), m);
}

fn log_error(m: &str) {
    println!("{} {} {}", format!("[{}]", ts()).red(), "✗".red(), m);
}

fn log_warn(m: &str) {
    println!(
        "{} {} {}",
        format!("[{}]", ts()).yellow(),
        "⚠".yellow(),
        m
    );
}

fn log_info(m: &str) {
    println!("{} {} {}", format!("[{}]", ts()).cyan(), "ℹ".cyan(), m);
}

fn log_key(m: &str) {
    println!("{} {}", format!("[{}]", ts()).blue(), m);
}

fn log_task(m: &str) {
    println!("{} {} {}", format!("[{}]", ts()).green(), "🎯", m);
}

fn log_iter(m: &str) {
    println!(
        "{} {} {}",
        format!("[{}]", ts()).magenta(),
        "🔄",
        m
    );
}

fn log_wait(m: &str) {
    println!(
        "{} {} {}",
        format!("[{}]", ts()).yellow(),
        "⏳",
        m
    );
}

fn log_todo(m: &str) {
    println!("{} {} {}", format!("[{}]", ts()).green(), "📋", m);
}

fn log_file(m: &str) {
    println!("{} {} {}", format!("[{}]", ts()).white(), "📄", m);
}

fn log_api(m: &str) {
    println!(
        "{} {} {}",
        format!("[{}]", ts()).magenta(),
        "🌐",
        m
    );
}

fn log_debug(m: &str, v: bool) {
    if v {
        println!(
            "{} {} {}",
            format!("[{}]", ts()).dimmed(),
            "🔍".dimmed(),
            m.dimmed()
        );
    }
}

fn log_prompt(p: &str, v: bool) {
    if v {
        println!(
            "\n{}\n{}\n{}\n",
            "═══ PROMPT ═══".cyan().bold(),
            p.dimmed(),
            "═══════════════".cyan()
        );
    }
}
// Tiempo de espera después de rate limit (segundos)
const WAIT_API_AFTER_LIMIT: u64 = 35;
// Tiempo mínimo entre requests al mismo proveedor
const WAIT_API_BETWEEN_REQUESTS: u64 = 2;

fn print_banner() {
    println!(
        "{}",
        r#"
  ╔══════════════════════════════════════════════════════════════════╗
  ║                                                                  ║
  ║   ██╗     ██╗   ██╗██╗███████╗███╗   ███╗██╗███╗   ██╗██████╗    ║
  ║   ██║     ██║   ██║██║██╔════╝████╗ ████║██║████╗  ██║██╔══██╗   ║
  ║   ██║     ██║   ██║██║███████╗██╔████╔██║██║██╔██╗ ██║██║  ██║   ║
  ║   ██║     ██║   ██║██║╚════██║██║╚██╔╝██║██║██║╚██╗██║██║  ██║   ║
  ║   ███████╗╚██████╔╝██║███████║██║ ╚═╝ ██║██║██║ ╚████║██████╔╝   ║
  ║   ╚══════╝ ╚═════╝ ╚═╝╚══════╝╚═╝     ╚═╝╚═╝╚═╝  ╚═══╝╚═════╝    ║
  ║                                                                  ║
  ║              🧠 Autonomous Development Agent v3.0 🧠             ║
  ║                         Your main brain                          ║
  ╚══════════════════════════════════════════════════════════════════╝
"#
        .cyan()
        .bold()
    );
    println!("                       v{} - {}\n", VERSION, BRAND);
}
// ══════════════════════════════════════════a═════════════════════════════════════
// FILE MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════════════

fn get_source_files(workspace: &Path) -> Vec<PathBuf> {
    fn visit(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let s = path.to_string_lossy();

            if s.contains("/.git") || s.contains("/target") || s.contains("/.aider") {
                continue;
            }

            if path.is_dir() {
                visit(&path, files);
            } else {
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();
                let name = path.file_name().unwrap_or_default().to_string_lossy();

                if ["rs", "py", "js", "ts", "go", "toml", "yaml", "json"]
                    .contains(&ext.as_str())
                    && !name.starts_with(".aider")
                    && !name.ends_with(".orig")
                {
                    files.push(path);
                }
            }
        }
    }

    let mut files = Vec::new();
    visit(workspace, &mut files);
    files.sort();
    files
}

fn count_tokens(path: &Path) -> usize {
    fs::read_to_string(path).map(|s| s.len() / 4).unwrap_or(0)
}

fn select_files(workspace: &Path, limit: usize) -> Vec<PathBuf> {
    let limit = limit * 75 / 100; // Use 75% of token limit
    let mut current = 0;
    let mut selected = Vec::new();

    // Priority files
    for p in [
        "src/main.rs",
        "src/lib.rs",
        "Cargo.toml",
        "main.py",
        "package.json",
    ] {
        let path = workspace.join(p);
        if path.exists() {
            let t = count_tokens(&path);
            if current + t <= limit {
                selected.push(path);
                current += t;
            }
        }
    }

    // Add other files
    for path in get_source_files(workspace) {
        if selected.contains(&path) {
            continue;
        }
        let t = count_tokens(&path);
        if current + t <= limit {
            selected.push(path);
            current += t;
        }
    }

    selected
}

// ═══════════════════════════════════════════════════════════════════════════════
// AIDER RESULT
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
enum AiderResult {
    Success {
        output: String,
        files_modified: Vec<String>,
        tokens_sent: usize,
        tokens_received: usize,
    },
    ApiError(ApiError),
    Error(String),
    Killed,
}

// ═══════════════════════════════════════════════════════════════════════════════
// WAIT UTILITY
// ═══════════════════════════════════════════════════════════════════════════════

fn wait_seconds(secs: u64) {
    log_wait(&format!("Waiting {} seconds...", secs));
    for remaining in (1..=secs).rev() {
        if should_exit() {
            return;
        }
        print!("\r  ⏳ {} sec...    ", remaining);
        std::io::stdout().flush().ok();
        thread::sleep(Duration::from_secs(1));
    }
    println!();
}

// ═══════════════════════════════════════════════════════════════════════════════
// OLLAMA INTEGRATION - DYNAMIC MODEL DISCOVERY & CONFIGURATION
// ═══════════════════════════════════════════════════════════════════════════════

/// Parse model size string to bytes
fn parse_model_size(size_str: &str) -> u64 {
    let lower = size_str.to_lowercase();
    
    // Extract number
    let num_str: String = lower.chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    
    let num: f64 = num_str.parse().unwrap_or(0.0);
    
    if lower.contains("gb") {
        (num * 1_000_000_000.0) as u64
    } else if lower.contains("mb") {
        (num * 1_000_000.0) as u64
    } else if lower.contains("kb") {
        (num * 1_000.0) as u64
    } else {
        num as u64
    }
}

/// Create a DynamicModel from Ollama model info
fn create_ollama_model(name: &str, size_bytes: u64) -> DynamicModel {
    let short_name = name.replace(":latest", "");
    let display_base = short_name.replace("-", " ").replace("_", " ");
    
    let estimated_params = if size_bytes > 0 {
        size_bytes as f64 / 550_000_000.0
    } else {
        estimate_params_from_name(&short_name)
    };
    
    // Usar la función que calcula config pero solo tomar token_limit
    let (token_limit, _gpu_layers) = calculate_ollama_config(size_bytes);
    
    let is_coder = short_name.to_lowercase().contains("code") 
        || short_name.to_lowercase().contains("coder");
    
    let display_name = if estimated_params > 0.5 {
        format!("{} ({:.1}B{}, local)", 
            display_base, 
            estimated_params,
            if is_coder { ", code" } else { "" }
        )
    } else {
        format!("{} (local)", display_base)
    };
    
    DynamicModel {
        name: short_name,
        display_name,
        provider: "ollama".to_string(),
        token_limit,
        is_free: true,
    }
}

/// Calculate optimal context window and GPU layers for your hardware
/// RTX 2060 Super (8GB VRAM) + 64GB RAM + i7-13700K
fn calculate_ollama_config(model_size_bytes: u64) -> (usize, usize) {
    let model_gb = model_size_bytes as f64 / 1_000_000_000.0;
    
    // Your hardware:
    // - VRAM: 8GB (RTX 2060 Super)
    // - RAM: 64GB
    // - CPU: i7-13700K (24 threads)
    
    // Strategy: 
    // 1. Load as much model as possible into VRAM
    // 2. Use remaining RAM for context (KV cache)
    // 3. KV cache uses ~2 bytes per token per layer
    
    // Typical layer counts: 7B=32, 13B=40, 34B=60, 70B=80
    let estimated_layers = match model_gb as u64 {
        0..=4 => 28,      // Small models
        5..=8 => 32,      // 7-8B models
        9..=15 => 40,     // 13-14B models
        16..=40 => 60,    // 32-34B models
        _ => 80,          // 70B+ models
    };
    
    // How many layers fit in 8GB VRAM?
    let layer_size_mb = (model_gb * 1000.0) / estimated_layers as f64;
    let max_gpu_layers = (7500.0 / layer_size_mb).floor() as usize; // Leave 500MB headroom
    let gpu_layers = max_gpu_layers.min(estimated_layers);
    
    // Calculate remaining memory for context
    // Model in RAM (if not all in VRAM): model_gb - (gpu_layers * layer_size_mb / 1000)
    let model_in_ram_gb = if gpu_layers >= estimated_layers {
        0.0
    } else {
        model_gb - (gpu_layers as f64 * layer_size_mb / 1000.0)
    };
    
    // Available RAM for context: 64GB - model_in_ram - 8GB system overhead
    let ram_for_context_gb = 64.0 - model_in_ram_gb - 8.0;
    
    // KV cache size: ~2 bytes per token per layer for both K and V
    // With 32 layers: ~128 bytes per token
    // With 40 layers: ~160 bytes per token
    let bytes_per_token = (estimated_layers * 4) as f64; // Simplified estimate
    
    let max_context = ((ram_for_context_gb * 1_000_000_000.0) / bytes_per_token) as usize;
    
    // Cap at reasonable limits and round to nice numbers
    let context = match max_context {
        0..=32_000 => 32_768,
        32_001..=65_000 => 65_536,
        65_001..=131_000 => 131_072,
        131_001..=262_000 => 262_144,
        262_001..=524_000 => 524_288,
        524_001..=800_000 => 786_432,  // ~768k
        _ => 1_048_576,                 // 1M max
    };
    
    (context, gpu_layers)
}

/// Get config from model name heuristics
fn get_ollama_config_from_name(model_name: &str) -> (usize, usize) {
    let lower = model_name.to_lowercase();
    
    // Based on common model sizes and your hardware
    if lower.contains("70b") || lower.contains("72b") {
        // 70B: Model mostly in RAM, limited context
        (65_536, 10)   // 64k context, 10 layers in GPU
    } else if lower.contains("34b") || lower.contains("32b") {
        // 34B: Partial GPU, good context
        (131_072, 15)  // 128k context, 15 layers in GPU
    } else if lower.contains("14b") || lower.contains("13b") {
        // 14B: Most in GPU, large context
        (262_144, 28)  // 256k context, 28 layers in GPU
    } else if lower.contains("7b") || lower.contains("8b") {
        // 7-8B: Full GPU, maximum context
        (524_288, 35)  // 512k context, all layers in GPU
    } else if lower.contains("3b") || lower.contains("4b") {
        // 3-4B: Full GPU, maximum context
        (786_432, 35)  // 768k context, all layers in GPU
    } else if lower.contains("1b") || lower.contains("2b") {
        // 1-2B: Full GPU, maximum context
        (1_048_576, 35) // 1M context, all layers in GPU
    } else {
        // Default: assume 7B-ish
        (262_144, 28)   // 256k context, 28 layers
    }
}


/// Create a custom Ollama model with specific context settings
fn create_custom_ollama_model(base_model: &str, context_size: usize) -> Option<String> {
    let custom_name = format!("{}-ctx{}k", 
        base_model.replace(":latest", "").replace(":", "-"),
        context_size / 1024
    );
    
    // Check if custom model already exists
    let models = get_ollama_models();
    if models.iter().any(|m| m.name == custom_name) {
        return Some(custom_name);
    }
    
    log_info(&format!("Creating custom model with {}k context...", context_size / 1024));
    
    // Create Modelfile
    let modelfile_content = format!(
        r#"FROM {}

PARAMETER num_ctx {}
PARAMETER num_gpu 99
PARAMETER num_thread 16
"#, 
        base_model, 
        context_size
    );
    
    // Write temporary Modelfile
    let modelfile_path = format!("/tmp/Modelfile.{}", custom_name);
    if fs::write(&modelfile_path, &modelfile_content).is_err() {
        log_error("Failed to create Modelfile");
        return None;
    }
    
    // Create the model
    let output = Command::new("ollama")
        .args(["create", &custom_name, "-f", &modelfile_path])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
    
    // Cleanup
    let _ = fs::remove_file(&modelfile_path);
    
    match output {
        Ok(status) if status.success() => {
            log_ok(&format!("✓ Created custom model: {}", custom_name));
            Some(custom_name)
        }
        _ => {
            log_error("Failed to create custom model");
            None
        }
    }
}

/// Get detailed info about an Ollama model
fn get_ollama_model_info(model_name: &str) -> Option<OllamaModelInfo> {
    let output = Command::new("ollama")
        .args(["show", model_name, "--modelfile"])
        .output()
        .ok()?;
    
    if !output.status.success() {
        return None;
    }
    
    let modelfile = String::from_utf8_lossy(&output.stdout);
    
    let mut info = OllamaModelInfo {
        name: model_name.to_string(),
        base_model: String::new(),
        parameters: HashMap::new(),
        size_bytes: 0,
    };
    
    for line in modelfile.lines() {
        let line = line.trim();
        
        if line.starts_with("FROM ") {
            info.base_model = line[5..].trim().to_string();
        } else if line.starts_with("PARAMETER ") {
            let parts: Vec<&str> = line[10..].split_whitespace().collect();
            if parts.len() >= 2 {
                info.parameters.insert(
                    parts[0].to_string(), 
                    parts[1..].join(" ")
                );
            }
        }
    }
    
    Some(info)
}

#[derive(Debug, Clone)]
struct OllamaModelInfo {
    name: String,
    base_model: String,
    parameters: HashMap<String, String>,
    size_bytes: u64,
}

/// List all Ollama models with detailed info
fn list_ollama_models_detailed() {
    let models = get_ollama_models();
    
    if models.is_empty() {
        log_warn("No Ollama models found");
        println!("\nTo install a model:");
        println!("  ollama pull qwen2.5-coder:7b");
        println!("  ollama pull codellama:13b");
        println!("  ollama pull deepseek-coder:6.7b");
        return;
    }
    
    println!("\n{}", "🦙 Local Ollama Models".cyan().bold());
    println!("{}", "═".repeat(90));
    println!("{:<35} {:>12} {:>12} {:>12}", 
        "MODEL", "SIZE", "CONTEXT", "GPU LAYERS");
    println!("{}", "─".repeat(90));
    
    for model in &models {
        let size_bytes = estimate_model_size_from_name(&model.name);
        let (ctx, gpu) = calculate_ollama_config(size_bytes);
        let size_gb = size_bytes as f64 / 1_000_000_000.0;
        
        println!("{:<35} {:>10.1} GB {:>10}k {:>12}",
            model.name.green(),
            size_gb,
            ctx / 1024,
            gpu
        );
    }
    
    println!("{}", "─".repeat(90));
    println!("\n{}", "Recommended models for coding:".yellow());
    println!("  qwen2.5-coder:7b     - Best balance of speed and quality");
    println!("  deepseek-coder:6.7b  - Great for complex code");
    println!("  codellama:13b        - Good for larger codebases");
    println!();
    println!("{}", "Usage:".cyan());
    println!("  rustmind -m qwen2.5-coder:7b -a 'your task'");
    println!("  rustmind -m ollama -a 'task'  # Uses first available");
    println!();
}

/// Validate Ollama model before use
fn validate_ollama_model(model_name: &str) -> Result<(), String> {
    // Check if Ollama is running
    if !is_ollama_running() {
        if !start_ollama() {
            return Err("Ollama is not running and failed to start".to_string());
        }
    }
    
    // Check if model exists
    let models = get_ollama_models();
    let found = models.iter().find(|m| 
        m.name == model_name || 
        m.name.starts_with(&format!("{}:", model_name)) ||
        model_name.starts_with(&m.name)
    );
    
    if found.is_none() {
        return Err(format!(
            "Model '{}' not found. Available: {}", 
            model_name,
            models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// SAFE DIFF PARSER - PROTECCIÓN CONTRA DIFFS DESTRUCTIVOS
// ═══════════════════════════════════════════════════════════════════════════════

/// Detecta si el output contiene diffs abreviados/fragmentados peligrosos
fn is_dangerous_abbreviated_diff(content: &str) -> bool {
    let dominated_patterns = [
        "// ... (omitting",
        "// ...(omitting", 
        "(omitting unchanged",
        "// ... unchanged",
        "# ... (rest",
        "// ... rest of",
        "// remaining unchanged",
        "/* ... */",
    ];
    
    let lower = content.to_lowercase();
    
    // Detectar patrones de omisión
    let has_omitting = dominated_patterns.iter().any(|p| lower.contains(&p.to_lowercase()));
    
    // Detectar múltiples hunks fragmentados para el mismo archivo
    let mut file_hunk_counts: HashMap<String, usize> = HashMap::new();
    let mut current_file = String::new();
    
    for line in content.lines() {
        let trimmed = line.trim();
        
        // Detectar nombre de archivo
        if (trimmed.ends_with(".rs") || trimmed.ends_with(".py") || trimmed.ends_with(".toml")
            || trimmed.ends_with(".js") || trimmed.ends_with(".ts"))
            && !trimmed.contains(' ')
            && !trimmed.starts_with('+')
            && !trimmed.starts_with('-')
            && !trimmed.starts_with("+++")
            && !trimmed.starts_with("---")
            && trimmed.len() < 100
        {
            current_file = trimmed.to_string();
        }
        
        // Contar hunks por archivo
        if trimmed.starts_with("@@") && !current_file.is_empty() {
            *file_hunk_counts.entry(current_file.clone()).or_insert(0) += 1;
        }
    }
    
    // Peligroso si un archivo tiene múltiples hunks fragmentados
    let has_fragmented = file_hunk_counts.values().any(|&count| count > 3);
    
    has_omitting || has_fragmented
}

/// Detecta si es un diff seguro tipo SEARCH/REPLACE completo
fn is_safe_search_replace(content: &str) -> bool {
    let search_count = content.matches("<<<<<<< SEARCH").count();
    let replace_count = content.matches(">>>>>>> REPLACE").count();
    let separator_count = content.matches("=======").count();
    
    // Debe tener bloques completos y balanceados
    search_count > 0 && search_count == replace_count && separator_count >= search_count
}
    
/// Procesa diffs de forma segura, rechazando código lazy
fn process_diffs_safely(workspace: &Path, output: &str, verbose: bool) -> Vec<String> {
    let mut modified_files = Vec::new();
    let mut rejected_files: Vec<String> = Vec::new();
    
    // Verificación inicial: si todo el output parece lazy, advertir
    let lazy_line_count = output.lines()
        .filter(|l| contains_lazy_pattern(l))
        .count();
    
    if lazy_line_count > 5 {
        log_warn(&format!("⚠️ Output contiene {} líneas con código omitido", lazy_line_count));
    }
    
    // Intentar SEARCH/REPLACE
    if is_safe_search_replace(output) {
        log_debug("Formato SEARCH/REPLACE detectado", verbose);
        let edits = extract_search_replace_edits(output);
        
        for edit in &edits {
            // Verificar si tiene código lazy
            if contains_lazy_pattern(&edit.replace) {
                log_warn(&format!("⚠️ Edit lazy detectado: {}", edit.filename));
                
                // Intentar reparar
                if let Some(repaired) = repair_lazy_edit(workspace, edit) {
                    if apply_single_edit_safely(workspace, &repaired, verbose) {
                        if !modified_files.contains(&edit.filename) {
                            modified_files.push(edit.filename.clone());
                        }
                        continue;
                    }
                }
                
                if let Some(repaired) = repair_lazy_edit_aggressive(workspace, edit) {
                    if apply_single_edit_safely(workspace, &repaired, verbose) {
                        if !modified_files.contains(&edit.filename) {
                            modified_files.push(edit.filename.clone());
                        }
                        continue;
                    }
                }
                
                // No se pudo reparar
                log_error(&format!("❌ RECHAZADO: {} (código lazy no reparable)", edit.filename));
                rejected_files.push(edit.filename.clone());
                continue;
            }
            
            // Edit normal - aplicar
            if apply_single_edit_safely(workspace, edit, verbose) {
                if !modified_files.contains(&edit.filename) {
                    modified_files.push(edit.filename.clone());
                }
            }
        }
        
        // Mostrar resumen de rechazados
        if !rejected_files.is_empty() {
            log_warn(&format!("⚠️ {} archivos rechazados por código lazy: {}", 
                rejected_files.len(), 
                rejected_files.join(", ")
            ));
        }
        
        return modified_files;
    }
    
    // Otros formatos de diff...
    if is_dangerous_abbreviated_diff(output) {
        log_warn("⚠️ Diffs fragmentados detectados - RECHAZANDO");
        return modified_files;
    }
    
    let unified_edits = extract_unified_diff_edits_improved(output);
    for edit in &unified_edits {
        if contains_lazy_pattern(&edit.replace) {
            log_error(&format!("❌ RECHAZADO: {} (código lazy)", edit.filename));
            rejected_files.push(edit.filename.clone());
            continue;
        }
        
        if apply_single_edit_safely(workspace, edit, verbose) {
            if !modified_files.contains(&edit.filename) {
                modified_files.push(edit.filename.clone());
            }
        }
    }
    
    if !rejected_files.is_empty() {
        log_warn(&format!("⚠️ {} archivos rechazados por código lazy", rejected_files.len()));
    }
    
    modified_files
}

/// Aplica un edit con detección y reparación de código lazy
fn apply_single_edit_safely(workspace: &Path, edit: &PendingEdit, verbose: bool) -> bool {
    let path = workspace.join(&edit.filename);
    
    if !path.exists() {
        log_debug(&format!("Archivo no existe: {}", edit.filename), verbose);
        return false;
    }
    
    let original = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            log_debug(&format!("Error leyendo {}: {}", edit.filename, e), verbose);
            return false;
        }
    };
    
    // ═══════════════════════════════════════════════════════════════
    // DETECTAR Y REPARAR CÓDIGO LAZY
    // ═══════════════════════════════════════════════════════════════
    let final_edit = if contains_lazy_pattern(&edit.replace) {
        log_warn(&format!("⚠️ Código lazy detectado en {}", edit.filename));
        
        // Intentar reparación normal primero
        if let Some(repaired) = repair_lazy_edit(workspace, edit) {
            repaired
        } 
        // Intentar reparación agresiva
        else if let Some(repaired) = repair_lazy_edit_aggressive(workspace, edit) {
            repaired
        }
        // No se pudo reparar - RECHAZAR
        else {
            log_error(&format!("❌ RECHAZADO {}: código lazy no reparable", edit.filename));
            log_info("   El modelo debe rehacer este cambio con código completo");
            return false;
        }
    } else {
        edit.clone()
    };
    
    let search = final_edit.search.trim();
    let replace = final_edit.replace.trim();
    
    // Verificar que el REPLACE no tenga lazy patterns
    if contains_lazy_pattern(replace) {
        log_error(&format!("❌ RECHAZADO {}: aún contiene código omitido", edit.filename));
        return false;
    }
    
    // Verificar que el search existe
    if !original.contains(search) {
        // Intentar fuzzy match
        if let Some(real_search) = find_fuzzy_match(&original, search) {
            let new_content = original.replace(&real_search, replace);
            
            if let Err(e) = validate_change_safety(&original, &new_content, &edit.filename) {
                log_warn(&format!("Cambio rechazado en {}: {}", edit.filename, e));
                return false;
            }
            
            if fs::write(&path, &new_content).is_ok() {
                let orig_lines = original.lines().count();
                let new_lines = new_content.lines().count();
                log_ok(&format!("✓ Aplicado (fuzzy): {} ({} → {} líneas)", 
                    edit.filename, orig_lines, new_lines));
                return true;
            }
        }
        
        log_debug(&format!("SEARCH no encontrado en {}", edit.filename), verbose);
        return false;
    }
    
    // Aplicar cambio
    let new_content = original.replace(search, replace);
    
    if let Err(e) = validate_change_safety(&original, &new_content, &edit.filename) {
        log_warn(&format!("Cambio rechazado en {}: {}", edit.filename, e));
        return false;
    }
    
    match fs::write(&path, &new_content) {
        Ok(_) => {
            let orig_lines = original.lines().count();
            let new_lines = new_content.lines().count();
            log_ok(&format!("✓ Aplicado: {} ({} → {} líneas)", 
                edit.filename, orig_lines, new_lines));
            true
        }
        Err(e) => {
            log_error(&format!("Error escribiendo {}: {}", edit.filename, e));
            false
        }
    }
}
/// Valida que el contenido no tenga código lazy antes de escribir

fn validate_no_lazy_code(content: &str, filename: &str) -> Result<(), String> {
    let lazy_lines: Vec<(usize, &str)> = content.lines()
        .enumerate()
        .filter(|(_, line)| contains_lazy_pattern(line))
        .collect();
    
    if lazy_lines.is_empty() {
        return Ok(());
    }
    
    let mut error_msg = format!("{}: Código lazy detectado en {} líneas:\n", filename, lazy_lines.len());
    
    for (line_num, line) in lazy_lines.iter().take(5) {
        error_msg.push_str(&format!("  L{}: {}\n", line_num + 1, line.trim()));
    }
    
    if lazy_lines.len() > 5 {
        error_msg.push_str(&format!("  ... y {} más\n", lazy_lines.len() - 5));
    }
    
    Err(error_msg)
}
/// Búsqueda fuzzy para encontrar el bloque real en el archivo
fn find_fuzzy_match(content: &str, search: &str) -> Option<String> {
    let search_lines: Vec<&str> = search.lines().collect();
    if search_lines.is_empty() {
        return None;
    }
    
    let content_lines: Vec<&str> = content.lines().collect();
    let first_search_line = search_lines[0].trim();
    
    // Buscar la primera línea
    for (i, line) in content_lines.iter().enumerate() {
        if line.trim() == first_search_line {
            // Verificar si las siguientes líneas coinciden
            let mut matches = true;
            let mut end_idx = i;
            
            for (j, search_line) in search_lines.iter().enumerate() {
                if i + j >= content_lines.len() {
                    matches = false;
                    break;
                }
                if content_lines[i + j].trim() != search_line.trim() {
                    matches = false;
                    break;
                }
                end_idx = i + j;
            }
            
            if matches {
                // Reconstruir el bloque original con whitespace original
                return Some(content_lines[i..=end_idx].join("\n"));
            }
        }
    }
    
    None
}

/// Extrae archivos mencionados en el output (para logging)
fn extract_mentioned_files(output: &str) -> Vec<String> {
    let mut files = Vec::new();
    
    for line in output.lines() {
        let trimmed = line.trim();
        
        // Detectar nombres de archivo
        let extensions = [".rs", ".py", ".toml", ".js", ".ts", ".go", ".yaml", ".json", ".md"];
        for ext in extensions {
            if trimmed.ends_with(ext) 
                && !trimmed.contains(' ') 
                && !trimmed.starts_with('+')
                && !trimmed.starts_with('-')
                && !trimmed.starts_with("+++")
                && !trimmed.starts_with("---")
                && trimmed.len() < 100
            {
                if !files.contains(&trimmed.to_string()) {
                    files.push(trimmed.to_string());
                }
            }
        }
    }
    
    files
}

// ═══════════════════════════════════════════════════════════════════════════════
// SEARCH/REPLACE PARSER - VERSIÓN MEJORADA
// ═══════════════════════════════════════════════════════════════════════════════

fn extract_search_replace_edits(output: &str) -> Vec<PendingEdit> {
    let mut edits = Vec::new();
    let mut current_file: Option<String> = None;
    let mut in_search = false;
    let mut in_replace = false;
    let mut search_content = String::new();
    let mut replace_content = String::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Detectar nombre de archivo
        if is_filename_line(trimmed) {
            // Guardar edit anterior si hay uno pendiente
            if let Some(ref file) = current_file {
                if in_replace && !search_content.is_empty() {
                    edits.push(PendingEdit {
                        filename: file.clone(),
                        search: search_content.clone(),
                        replace: replace_content.clone(),
                    });
                }
            }
            current_file = Some(trimmed.to_string());
            in_search = false;
            in_replace = false;
            search_content.clear();
            replace_content.clear();
            continue;
        }

        // Inicio de SEARCH
        if line.contains("<<<<<<< SEARCH") {
            in_search = true;
            in_replace = false;
            search_content.clear();
            continue;
        }
        
        // Separador entre SEARCH y REPLACE
        if line.contains("=======") && in_search {
            in_search = false;
            in_replace = true;
            replace_content.clear();
            continue;
        }
        
        // Fin de REPLACE
        if line.contains(">>>>>>> REPLACE") {
            if let Some(ref file) = current_file {
                if !search_content.is_empty() {
                    edits.push(PendingEdit {
                        filename: file.clone(),
                        search: search_content.clone(),
                        replace: replace_content.clone(),
                    });
                }
            }
            in_search = false;
            in_replace = false;
            search_content.clear();
            replace_content.clear();
            continue;
        }

        // Acumular contenido
        if in_search {
            search_content.push_str(line);
            search_content.push('\n');
        } else if in_replace {
            replace_content.push_str(line);
            replace_content.push('\n');
        }
    }

    edits
}

// ═══════════════════════════════════════════════════════════════════════════════
// UNIFIED DIFF PARSER - VERSIÓN MEJORADA
// ═══════════════════════════════════════════════════════════════════════════════

fn extract_unified_diff_edits(output: &str) -> Vec<PendingEdit> {
    let mut edits = Vec::new();
    let mut current_file: Option<String> = None;
    let mut minus_lines = Vec::new();
    let mut plus_lines = Vec::new();
    let mut context_lines = Vec::new();
    let mut in_hunk = false;

    for line in output.lines() {
        // Detectar archivo en formato unified diff
        if line.starts_with("--- a/") {
            if let Some(ref file) = current_file {
                if !minus_lines.is_empty() || !plus_lines.is_empty() {
                    edits.push(create_edit_from_diff(file, &minus_lines, &plus_lines, &context_lines));
                }
            }
            minus_lines.clear();
            plus_lines.clear();
            context_lines.clear();
            
            let path = line.trim_start_matches("--- a/").trim();
            if !path.is_empty() && !path.starts_with("/dev/null") {
                current_file = Some(path.to_string());
            }
            continue;
        }

        if line.starts_with("+++ b/") {
            let path = line.trim_start_matches("+++ b/").trim();
            if !path.is_empty() && !path.starts_with("/dev/null") {
                current_file = Some(path.to_string());
            }
            continue;
        }

        // Inicio de hunk
        if line.starts_with("@@") {
            if let Some(ref file) = current_file {
                if !minus_lines.is_empty() || !plus_lines.is_empty() {
                    edits.push(create_edit_from_diff(file, &minus_lines, &plus_lines, &context_lines));
                }
            }
            minus_lines.clear();
            plus_lines.clear();
            context_lines.clear();
            in_hunk = true;
            continue;
        }

        if in_hunk {
            if line.starts_with('-') && !line.starts_with("---") {
                minus_lines.push(line[1..].to_string());
                context_lines.push(line[1..].to_string());
            } else if line.starts_with('+') && !line.starts_with("+++") {
                plus_lines.push(line[1..].to_string());
            } else if line.starts_with(' ') {
                let ctx = if line.len() > 1 { &line[1..] } else { "" };
                context_lines.push(ctx.to_string());
            } else if line.trim().is_empty() {
                // Línea vacía puede ser parte del contexto
            } else {
                // Fin del hunk
                in_hunk = false;
            }
        }
    }

    // Guardar último hunk
    if let Some(ref file) = current_file {
        if !minus_lines.is_empty() || !plus_lines.is_empty() {
            edits.push(create_edit_from_diff(file, &minus_lines, &plus_lines, &context_lines));
        }
    }

    edits
}

fn create_edit_from_diff(
    filename: &str, 
    minus: &[String], 
    plus: &[String], 
    context: &[String]
) -> PendingEdit {
    // Usar contexto para crear un SEARCH más preciso
    let mut search_lines: Vec<String> = Vec::new();
    
    // Agregar líneas de contexto antes (para mejor matching)
    let context_before: Vec<&String> = context.iter()
        .take(3)  // Hasta 3 líneas de contexto
        .collect();
    
    for ctx_line in &context_before {
        if !ctx_line.trim().is_empty() {
            search_lines.push(ctx_line.to_string());
        }
    }
    
    // Agregar líneas a reemplazar (minus)
    for line in minus {
        search_lines.push(line.clone());
    }
    
    // Agregar contexto después si hay
    let context_after: Vec<&String> = context.iter()
        .skip(3)
        .take(3)
        .collect();
    
    for ctx_line in &context_after {
        if !ctx_line.trim().is_empty() {
            search_lines.push(ctx_line.to_string());
        }
    }
    
    // Construir REPLACE con contexto
    let mut replace_lines: Vec<String> = Vec::new();
    
    // Mismo contexto antes
    for ctx_line in &context_before {
        if !ctx_line.trim().is_empty() {
            replace_lines.push(ctx_line.to_string());
        }
    }
    
    // Nuevas líneas (plus)
    for line in plus {
        replace_lines.push(line.clone());
    }
    
    // Mismo contexto después
    for ctx_line in &context_after {
        if !ctx_line.trim().is_empty() {
            replace_lines.push(ctx_line.to_string());
        }
    }
    
    PendingEdit {
        filename: filename.to_string(),
        search: search_lines.join("\n"),
        replace: replace_lines.join("\n"),
    }
}

/// Extrae un bloque completo (función, struct, etc.) del código
fn extract_complete_block(content: &str, start_keyword: &str) -> Option<String> {
    let start_pos = content.find(start_keyword)?;
    
    let mut brace_depth = 0;
    let mut in_block = false;
    let mut in_string = false;
    let mut escape_next = false;
    let mut end_pos = start_pos;
    
    let chars: Vec<char> = content.chars().collect();
    
    for i in start_pos..chars.len() {
        let c = chars[i];
        
        if escape_next {
            escape_next = false;
            continue;
        }
        
        if c == '\\' {
            escape_next = true;
            continue;
        }
        
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        
        if in_string {
            continue;
        }
        
        match c {
            '{' => {
                brace_depth += 1;
                in_block = true;
            }
            '}' => {
                brace_depth -= 1;
                if in_block && brace_depth == 0 {
                    end_pos = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    
    if end_pos > start_pos {
        Some(content[start_pos..end_pos].to_string())
    } else {
        None
    }
}

/// Parsea datos estructurados del output
fn parse_structured_output(output: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let mut current_section = String::new();
    let mut section_content = String::new();
    let mut in_data = false;
    
    for line in output.lines() {
        let trimmed = line.trim();
        
        // Detectar inicio de sección
        if trimmed.ends_with(':') && trimmed.len() < 50 {
            // Guardar sección anterior si existe
            if !current_section.is_empty() && !section_content.is_empty() {
                result.insert(current_section.clone(), section_content.trim().to_string());
            }
            
            current_section = trimmed.trim_end_matches(':').to_string();
            section_content.clear();
            in_data = true;
        } else if in_data {
            // Detectar fin de sección (línea vacía o nueva sección)
            if trimmed.is_empty() && !section_content.is_empty() {
                // Posible fin de sección, pero seguir acumulando
            } else {
                section_content.push_str(line);
                section_content.push('\n');
            }
        }
    }
    
    // Guardar última sección
    if !current_section.is_empty() && !section_content.is_empty() {
        result.insert(current_section, section_content.trim().to_string());
    }
    
    result
}
// ═══════════════════════════════════════════════════════════════════════════════
// OLLAMA - DETECCIÓN DINÁMICA DE MODELOS LOCALES
// ═══════════════════════════════════════════════════════════════════════════════

/// Parsea respuesta JSON de Ollama API
fn parse_ollama_api_response(json: &str) -> Vec<DynamicModel> {
    let mut models = Vec::new();
    
    // Parser simple sin serde
    // Formato: {"models":[{"name":"qwen-coder-max:latest","size":6789012345,...},...]}
    
    let mut in_models_array = false;
    let mut current_name = String::new();
    let mut current_size: u64 = 0;
    let mut brace_depth = 0;
    
    for line in json.split(&['{', '}', ','][..]) {
        let line = line.trim();
        
        if line.contains("\"models\"") {
            in_models_array = true;
            continue;
        }
        
        if !in_models_array {
            continue;
        }
        
        // Extraer name
        if line.contains("\"name\"") {
            if let Some(start) = line.find(":") {
                let value = line[start + 1..].trim().trim_matches('"').trim_matches(':');
                let clean_value = value.trim_matches(|c| c == '"' || c == ' ' || c == ':');
                if !clean_value.is_empty() && clean_value.contains(':') || clean_value.len() > 2 {
                    current_name = clean_value.replace("\"", "").trim().to_string();
                }
            }
        }
        
        // Extraer size
        if line.contains("\"size\"") {
            if let Some(start) = line.find(":") {
                let value = line[start + 1..].trim();
                let num_str: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
                current_size = num_str.parse().unwrap_or(0);
            }
        }
        
        // Si tenemos un nombre válido, crear el modelo
        if !current_name.is_empty() && (current_size > 0 || line.contains("modified_at")) {
            models.push(create_ollama_dynamic_model(&current_name, current_size));
            current_name.clear();
            current_size = 0;
        }
    }
    
    // Cleanup: remover duplicados
    models.sort_by(|a, b| a.name.cmp(&b.name));
    models.dedup_by(|a, b| a.name == b.name);
    
    models
}

/// Obtiene modelos via CLI de Ollama
fn get_ollama_models_via_cli() -> Vec<DynamicModel> {
    let output = match Command::new("ollama").arg("list").output() {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Formato de ollama list:
    // NAME                       ID              SIZE      MODIFIED     
    // qwen-coder-max:latest      9e1e2f809841    6.3 GB    20 hours ago
    
    stdout
        .lines()
        .skip(1) // Saltar header
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            
            // Parsear por columnas (espacios múltiples como separador)
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                return None;
            }
            
            let name = parts[0].to_string();
            
            // Buscar tamaño (formato: "X.X GB" o "XXX MB")
            let mut size_bytes: u64 = 0;
            for (i, part) in parts.iter().enumerate() {
                if *part == "GB" && i > 0 {
                    if let Ok(num) = parts[i - 1].parse::<f64>() {
                        size_bytes = (num * 1_000_000_000.0) as u64;
                        break;
                    }
                }
                if *part == "MB" && i > 0 {
                    if let Ok(num) = parts[i - 1].parse::<f64>() {
                        size_bytes = (num * 1_000_000.0) as u64;
                        break;
                    }
                }
            }
            
            Some(create_ollama_dynamic_model(&name, size_bytes))
        })
        .collect()
}

/// Calcula configuración óptima para Ollama en tu hardware
/// RTX 2060 Super (8GB) + 64GB RAM + i7-13700K
fn calculate_optimal_ollama_config(size_bytes: u64, name: &str) -> (usize, usize) {
    let model_gb = if size_bytes > 0 {
        size_bytes as f64 / 1_000_000_000.0
    } else {
        estimate_params_from_name(name) * 0.55 // Q4 estimate
    };
    
    // Tu hardware:
    // - VRAM: 8GB disponibles (~7GB usables)
    // - RAM: 64GB
    // - CPU: i7-13700K (24 threads)
    
    // Estrategia: maximizar contexto mientras mantenemos modelo en memoria
    
    let (context, gpu_layers) = if model_gb <= 4.0 {
        // Modelos pequeños (<4GB): todo en GPU, contexto masivo
        (524_288, 99)  // 512k tokens, todas las capas
    } else if model_gb <= 7.0 {
        // Modelos 7B (~4-7GB): casi todo en GPU
        (393_216, 35)  // 384k tokens
    } else if model_gb <= 10.0 {
        // Modelos 8-10GB: split GPU/RAM
        (262_144, 28)  // 256k tokens
    } else if model_gb <= 20.0 {
        // Modelos grandes (13-20GB): mayormente RAM
        (131_072, 20)  // 128k tokens
    } else if model_gb <= 40.0 {
        // Modelos muy grandes (32-40GB)
        (65_536, 12)   // 64k tokens
    } else {
        // Modelos enormes (70B+)
        (32_768, 8)    // 32k tokens
    };
    
    (context, gpu_layers)
}

/// Stop Ollama server
fn stop_ollama() {
    let _ = Command::new("pkill").args(["-f", "ollama"]).output();
}

// ═══════════════════════════════════════════════════════════════════════════════
// GEMINI MODELS - DETECCIÓN MEJORADA INCLUYENDO PRO
// ═══════════════════════════════════════════════════════════════════════════════


fn parse_gemini_models(json: &str) -> Vec<DynamicModel> {
    let mut models = Vec::new();
    let mut seen = HashSet::new();

    // Buscar en el JSON todos los modelos
    for line in json.lines() {
        // Buscar "name": "models/gemini-..."
        if line.contains("\"name\"") && line.contains("models/gemini") {
            if let Some(start) = line.find("models/gemini") {
                let rest = &line[start..];
                if let Some(end) = rest.find('"') {
                    let model_path = &rest[..end];
                    let model_name = model_path.replace("models/", "");
                    
                    // Filtrar modelos no útiles para código
                    if model_name.contains("embedding")
                        || model_name.contains("aqa")
                        || model_name.contains("imagen")
                        || model_name.contains("vision")
                        || model_name.contains("1.0")
                        || model_name.contains("bison")
                    {
                        continue;
                    }
                    
                    // Evitar duplicados
                    let base_name = model_name
                        .replace("-latest", "")
                        .replace("-001", "")
                        .replace("-002", "");
                    
                    if seen.contains(&base_name) {
                        continue;
                    }
                    seen.insert(base_name.clone());
                    
                    // Determinar características
                    let (token_limit, is_free) = if model_name.contains("pro") {
                        (2_000_000, false) // Pro: 2M context, paid
                    } else if model_name.contains("flash") {
                        (1_000_000, true)  // Flash: 1M context, free tier
                    } else {
                        (200_000, true)    // Default
                    };
                    
                    let display = base_name
                        .replace("gemini-", "Gemini ")
                        .replace("-exp", " (exp)")
                        .replace("-preview", " preview")
                        .replace("-", " ");
                    
                    models.push(DynamicModel {
                        name: base_name,
                        display_name: display,
                        provider: "gemini".to_string(),
                        token_limit,
                        is_free,
                    });
                }
            }
        }
    }

    models
}

// ═══════════════════════════════════════════════════════════════════════════════
// GIT INTEGRATION - COMMITS ROBUSTOS Y HISTORIAL DETALLADO
// ═══════════════════════════════════════════════════════════════════════════════


#[derive(Debug, Clone)]
struct GitCommit {
    hash: String,
    short_hash: String,
    date: String,
    time: String,
    message: String,
    files_changed: Vec<String>,
}

/// Verifica si es un repositorio git
fn is_git_repo(workspace: &Path) -> bool {
    workspace.join(".git").exists()
}

/// Inicializa git repo si no existe
fn ensure_git_repo(workspace: &Path) -> bool {
    if is_git_repo(workspace) {
        return true;
    }

    log_info("Inicializando repositorio git...");
    
    let init = Command::new("git")
        .args(["init"])
        .current_dir(workspace)
        .output();

    match init {
        Ok(o) if o.status.success() => {
            log_ok("✓ Repositorio git inicializado");
            
            // Crear .gitignore básico
            let gitignore = workspace.join(".gitignore");
            if !gitignore.exists() {
                let _ = fs::write(&gitignore, "target/\n.aider*\n*.orig\n.env\n");
            }
            
            // Commit inicial
            let _ = Command::new("git")
                .args(["add", "-A"])
                .current_dir(workspace)
                .output();
            
            let _ = Command::new("git")
                .args(["commit", "-m", "Initial commit by RustMind", "--allow-empty"])
                .current_dir(workspace)
                .output();
            
            true
        }
        _ => {
            log_warn("No se pudo inicializar git");
            false
        }
    }
}

/// Commit robusto con reintentos
fn git_commit_changes(workspace: &Path, message: &str) -> bool {
    if !is_git_repo(workspace) {
        return false;
    }

    // Stage all changes
    let add = Command::new("git")
        .args(["add", "-A"])
        .current_dir(workspace)
        .output();

    if add.is_err() {
        return false;
    }

    // Verificar si hay cambios
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(workspace)
        .output();

    let has_changes = status
        .as_ref()
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);

    if !has_changes {
        return true; // No hay cambios pero no es error
    }

    // Hacer commit
    let commit = Command::new("git")
        .args(["commit", "-m", message, "--no-verify"])
        .current_dir(workspace)
        .output();

    match commit {
        Ok(o) => {
            if o.status.success() {
                log_debug(&format!("📦 Git commit: {}", message), false);
                true
            } else {
                log_debug(&format!("Git commit failed: {}", String::from_utf8_lossy(&o.stderr)), true);
                false
            }
        }
        Err(_) => false,
    }
}

/// Commit antes de cada iteración (backup de seguridad)
fn commit_before_iteration(workspace: &Path, iteration: usize) -> bool {
    let msg = format!("rustmind: backup pre-iteración {}", iteration);
    git_commit_changes(workspace, &msg)
}

/// Commit después de cambios exitosos con lista de archivos
fn commit_after_changes(workspace: &Path, files: &[String], iteration: usize) -> bool {
    if files.is_empty() {
        return false;
    }

    // Crear mensaje descriptivo con archivos
    let files_str = if files.len() > 4 {
        format!("{}, +{} más", files[..4].join(", "), files.len() - 4)
    } else {
        files.join(", ")
    };

    let msg = format!("rustmind: iter {} - {}", iteration, files_str);
    git_commit_changes(workspace, &msg)
}

/// Obtiene historial de commits con archivos modificados
fn get_git_commits_detailed(workspace: &Path, limit: usize) -> Vec<GitCommit> {
    // Primero obtener commits básicos
    let output = Command::new("git")
        .args([
            "log",
            "--format=%H|%h|%ad|%s",
            "--date=format:%Y-%m-%d|%H:%M:%S",
            &format!("-{}", limit),
        ])
        .current_dir(workspace)
        .output();

    let commits: Vec<GitCommit> = match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|line| {
                    let parts: Vec<&str> = line.splitn(5, '|').collect();
                    if parts.len() >= 5 {
                        Some(GitCommit {
                            hash: parts[0].to_string(),
                            short_hash: parts[1].to_string(),
                            date: parts[2].to_string(),
                            time: parts[3].to_string(),
                            message: parts[4].to_string(),
                            files_changed: Vec::new(), // Se llena después
                        })
                    } else {
                        None
                    }
                })
                .collect()
        }
        _ => Vec::new(),
    };

    // Obtener archivos modificados para cada commit
    commits.into_iter().map(|mut commit| {
        let files_output = Command::new("git")
            .args(["diff-tree", "--no-commit-id", "--name-only", "-r", &commit.hash])
            .current_dir(workspace)
            .output();

        if let Ok(o) = files_output {
            if o.status.success() {
                commit.files_changed = String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(|s| s.to_string())
                    .collect();
            }
        }
        commit
    }).collect()
}

/// Muestra historial de commits con formato detallado
fn show_git_commits(workspace: &Path) {
    let commits = get_git_commits_detailed(workspace, 30);

    if commits.is_empty() {
        log_warn("No hay commits en este repositorio");
        println!("\nPara inicializar: git init && git add -A && git commit -m 'init'");
        return;
    }

    println!("\n{}", "📜 Historial de Commits".white().bold());
    println!("{}", "═".repeat(95));
    println!(
        "{:<4} {:<12} {:<10} {:<40} {}",
        "#", "FECHA", "HORA", "MENSAJE", "ARCHIVOS"
    );
    println!("{}", "─".repeat(95));

    for (i, commit) in commits.iter().enumerate() {
        let num = format!("{}", i + 1);
        
        // Truncar mensaje de forma segura
        let msg = truncate_safe(&commit.message, 38);
        
        // Colorear por tipo - CORREGIDO: sin punto y coma dentro
        let msg_colored = if commit.message.starts_with("rustmind:") || commit.message.starts_with("luismind:") {
            msg.yellow().to_string()
        } else if commit.message.starts_with("aider:") {
            msg.cyan().to_string()
        } else {
            msg.white().to_string()
        };

        // Archivos modificados - CORREGIDO: usar join_first_n
        let files_str = if commit.files_changed.is_empty() {
            "-".dimmed().to_string()
        } else if commit.files_changed.len() > 2 {
            format!("{} +{}", 
                join_first_n(&commit.files_changed, 2, ", "), 
                commit.files_changed.len() - 2
            ).dimmed().to_string()
        } else {
            commit.files_changed.join(", ").dimmed().to_string()
        };

        println!(
            "{:<4} {:<12} {:<10} {:<40} {}",
            num.cyan(),
            commit.date.dimmed(),
            commit.time.dimmed(),
            msg_colored,
            files_str
        );
    }

    println!("{}", "─".repeat(95));
    println!();
    println!("{}", "Uso:".cyan().bold());
    println!("  {} -u 5        # Volver al commit #5", APP_NAME);
    println!("  {} -u 1        # Volver al commit más reciente", APP_NAME);
    println!("  git diff HEAD~1      # Ver cambios del último commit");
    println!("  git show <hash>      # Ver detalles de un commit");
    println!();
}

/// Deshace al commit especificado con backup de seguridad
fn undo_to_commit(workspace: &Path, commit_num: usize) -> bool {
    let commits = get_git_commits_detailed(workspace, 50);

    if commits.is_empty() {
        log_error("No hay commits disponibles");
        return false;
    }

    if commit_num == 0 || commit_num > commits.len() {
        log_error(&format!(
            "Número de commit inválido. Rango: 1-{}",
            commits.len()
        ));
        return false;
    }

    let target = &commits[commit_num - 1];

    println!();
    log_warn(&format!("⚠️  Vas a revertir al commit #{}", commit_num));
    println!("    {} {} ({})", 
        target.short_hash.green(), 
        target.message.yellow(),
        target.date.dimmed()
    );
    
    if !target.files_changed.is_empty() {
        println!("    Archivos en ese commit: {}", target.files_changed.join(", ").dimmed());
    }
    println!();

    // Backup de seguridad ANTES del reset
    let backup_msg = format!("rustmind: backup antes de undo a {}", target.short_hash);
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(workspace)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", &backup_msg, "--allow-empty", "--no-verify"])
        .current_dir(workspace)
        .output();

    // Hard reset al commit target
    let output = Command::new("git")
        .args(["reset", "--hard", &target.hash])
        .current_dir(workspace)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            log_ok(&format!(
                "✓ Revertido al commit #{}: {}",
                commit_num, target.short_hash
            ));
            log_info("Para recuperar: git reflog → git reset --hard <hash>");
            true
        }
        Ok(o) => {
            log_error(&format!("Error: {}", String::from_utf8_lossy(&o.stderr)));
            false
        }
        Err(e) => {
            log_error(&format!("Error de git: {}", e));
            false
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// HELPER: Verificar Ollama running
// ═══════════════════════════════════════════════════════════════════════════════

fn start_ollama() -> bool {
    if is_ollama_running() {
        return true;
    }
    
    log_info("Iniciando Ollama...");
    
    let _ = Command::new("ollama")
        .arg("serve")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    
    for i in 1..=10 {
        thread::sleep(Duration::from_secs(1));
        if is_ollama_running() {
            log_ok(&format!("✓ Ollama iniciado ({}s)", i));
            return true;
        }
    }
    
    log_warn("Ollama no responde");
    false
}

/// Estimate model size from name (when API info not available)
fn estimate_model_size_from_name(name: &str) -> u64 {
    let lower = name.to_lowercase();
    
    // Extract size indicator
    if lower.contains("70b") || lower.contains("72b") {
        42_000_000_000 // ~42GB for Q4
    } else if lower.contains("34b") || lower.contains("32b") {
        20_000_000_000 // ~20GB for Q4
    } else if lower.contains("14b") || lower.contains("13b") {
        8_000_000_000  // ~8GB for Q4
    } else if lower.contains("7b") || lower.contains("8b") {
        4_500_000_000  // ~4.5GB for Q4
    } else if lower.contains("3b") || lower.contains("4b") {
        2_500_000_000  // ~2.5GB for Q4
    } else if lower.contains("1b") || lower.contains("2b") {
        1_500_000_000  // ~1.5GB for Q4
    } else {
        // Default to 7B size
        4_500_000_000
    }
}
/// Helper para formatear nombres de modelos
fn format_model_display_name(id: &str) -> String {
    id.replace("-", " ")
      .replace("_", " ")
      .split_whitespace()
      .map(|w| {
          let mut c = w.chars();
          match c.next() {
              None => String::new(),
              Some(f) => f.to_uppercase().chain(c).collect(),
          }
      })
      .collect::<Vec<_>>()
      .join(" ")
}

fn parse_gemini_models_complete(json: &str) -> Vec<DynamicModel> {
    let mut models = Vec::new();
    let mut seen_names = HashSet::new();

    // Buscar todos los modelos gemini en el JSON
    let mut i = 0;
    let chars: Vec<char> = json.chars().collect();
    
    while i < chars.len() {
        // Buscar "name": "models/gemini
        if i + 20 < chars.len() {
            let window: String = chars[i..i+20].iter().collect();
            if window.contains("\"name\"") && json[i..].contains("models/gemini") {
                // Encontrar el valor completo
                if let Some(start) = json[i..].find("models/") {
                    let abs_start = i + start;
                    if let Some(end) = json[abs_start..].find('"') {
                        let model_path = &json[abs_start..abs_start + end];
                        let model_name = model_path.replace("models/", "");
                        
                        // Filtrar solo modelos no útiles para código
                        let dominated_filters = ["embedding", "aqa", "imagen", "vision-preview"];
                        let should_skip = dominated_filters.iter().any(|f| model_name.contains(f));
                        
                        if !should_skip && !seen_names.contains(&model_name) {
                            seen_names.insert(model_name.clone());
                            
                            // Determinar características del modelo
                            let (token_limit, is_free) = determine_gemini_model_specs(&model_name);
                            
                            // Crear display name legible
                            let display = create_gemini_display_name(&model_name);
                            
                            models.push(DynamicModel {
                                name: model_name.clone(),
                                display_name: display,
                                provider: "gemini".to_string(),
                                token_limit,
                                is_free,
                            });
                        }
                        
                        i = abs_start + end;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }

    // Ordenar por nombre para consistencia
    models.sort_by(|a, b| {
        // Ordenar: primero por versión (2.5 > 2.0 > 1.5), luego por tipo (pro > flash)
        let version_a = extract_gemini_version(&a.name);
        let version_b = extract_gemini_version(&b.name);
        
        match version_b.partial_cmp(&version_a) {
            Some(std::cmp::Ordering::Equal) | None => {
                // Si misma versión, pro antes que flash
                let is_pro_a = a.name.contains("pro");
                let is_pro_b = b.name.contains("pro");
                is_pro_b.cmp(&is_pro_a).then(a.name.cmp(&b.name))
            }
            Some(ord) => ord,
        }
    });

    models
}


/// Extrae versión numérica de nombre de modelo Gemini
fn extract_gemini_version(name: &str) -> f32 {
    if name.contains("2.5") { 2.5 }
    else if name.contains("2.0") { 2.0 }
    else if name.contains("1.5") { 1.5 }
    else if name.contains("1.0") { 1.0 }
    else { 0.0 }
}

/// Determina especificaciones del modelo Gemini por nombre
fn determine_gemini_model_specs(name: &str) -> (usize, bool) {
    let is_pro = name.contains("pro");
    let is_flash = name.contains("flash");
    let is_lite = name.contains("lite");
    
    let token_limit = if is_pro {
        2_000_000  // Pro: 2M context
    } else if is_lite {
        500_000    // Lite: 500k
    } else if is_flash {
        1_000_000  // Flash: 1M
    } else {
        200_000    // Default
    };
    
    // Flash y Lite son gratuitos en tier free
    let is_free = is_flash || is_lite;
    
    (token_limit, is_free)
}

/// Crea nombre display legible para modelo Gemini
fn create_gemini_display_name(name: &str) -> String {
    name
        .replace("gemini-", "Gemini ")
        .replace("-exp", " (experimental)")
        .replace("-preview", " Preview")
        .replace("-latest", "")
        .replace("-", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Obtiene modelos de OpenRouter dinámicamente
fn fetch_openrouter_models() -> Vec<DynamicModel> {
    let output = Command::new("curl")
        .args([
            "-s",
            "--connect-timeout", "10",
            "https://openrouter.ai/api/v1/models",
        ])
        .output();

    match output {
        Ok(o) if o.status.success() => parse_openrouter_models_complete(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    }
}

fn parse_openrouter_models_complete(json: &str) -> Vec<DynamicModel> {
    let mut models = Vec::new();
    let mut current_id = String::new();
    let mut current_context: usize = 128_000;
    let mut is_free = false;
    let mut in_data = false;

    for line in json.split(&['{', '}'][..]) {
        let line = line.trim();
        
        if line.contains("\"data\"") {
            in_data = true;
            continue;
        }
        
        // Extraer id
        if line.contains("\"id\"") {
            if let Some(start) = line.find("\"id\"") {
                let rest = &line[start..];
                if let Some(colon) = rest.find(':') {
                    let value_part = rest[colon + 1..].trim();
                    if let Some(end) = value_part.find(',').or(Some(value_part.len())) {
                        current_id = value_part[..end]
                            .trim()
                            .trim_matches('"')
                            .to_string();
                    }
                }
            }
        }
        
        // Extraer token_limit
        if line.contains("\"token_limit\"") {
            if let Some(start) = line.find(":") {
                let value = line[start + 1..].trim();
                let num_str: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
                if let Ok(ctx) = num_str.parse::<usize>() {
                    current_context = ctx;
                }
            }
        }
        
        // Detectar si es gratis
        if line.contains("\":free\"") || line.contains("\"pricing\"") && line.contains("\"0\"") {
            is_free = true;
        }
        
        // Si tenemos un ID válido y es un modelo útil para código
        if !current_id.is_empty() && is_useful_openrouter_model(&current_id) {
            let display = current_id
                .split('/')
                .last()
                .unwrap_or(&current_id)
                .replace("-", " ")
                .replace("_", " ");
            
            let short_name = current_id
                .split('/')
                .last()
                .unwrap_or(&current_id)
                .to_string();
            
            models.push(DynamicModel {
                name: short_name,
                display_name: format!("{} (OpenRouter)", display),
                provider: "openrouter".to_string(),
                token_limit: current_context,
                is_free,
            });
            
            current_id.clear();
            current_context = 128_000;
            is_free = false;
        }
    }

    models.sort_by(|a, b| a.name.cmp(&b.name));
    models.dedup_by(|a, b| a.name == b.name);
    
    models
}

/// Filtra modelos útiles de OpenRouter (excluye imagen, audio, etc)
fn is_useful_openrouter_model(id: &str) -> bool {
    let dominated_lower = id.to_lowercase();
    
    // Incluir solo modelos de texto/código
    let dominated_good = ["claude", "gpt", "gemini", "llama", "mistral", "mixtral", 
                 "qwen", "deepseek", "codestral", "phi", "wizardlm"];
    
    let has_good = dominated_good.iter().any(|g| dominated_lower.contains(g));
    
    // Excluir modelos de imagen/audio/vision
    let dominated_bad = ["vision", "image", "audio", "whisper", "dall", "stable", "flux"];
    let has_bad = dominated_bad.iter().any(|b| dominated_lower.contains(b));
    
    has_good && !has_bad
}

fn create_ollama_dynamic_model(name: &str, size_bytes: u64) -> DynamicModel {
    let short_name = name.replace(":latest", "");
    let display_base = short_name.replace("-", " ").replace("_", " ");
    
    // Estimar parámetros
    let estimated_params = if size_bytes > 0 {
        size_bytes as f64 / 550_000_000.0
    } else {
        estimate_params_from_name(&short_name)
    };
    
    // Calcular contexto óptimo para tu hardware (64GB RAM, 8GB VRAM)
    let token_limit = calculate_optimal_context(size_bytes, &short_name);
    
    let display_name = if estimated_params > 0.5 {
        format!("{} ({:.1}B, local)", display_base, estimated_params)
    } else {
        format!("{} (local)", display_base)
    };
    
    DynamicModel {
        name: short_name,
        display_name,
        provider: "ollama".to_string(),
        token_limit,
        is_free: true,
    }
}

fn estimate_params_from_name(name: &str) -> f64 {
    let lower = name.to_lowercase();
    
    if lower.contains("70b") || lower.contains("72b") { 70.0 }
    else if lower.contains("34b") || lower.contains("32b") { 34.0 }
    else if lower.contains("14b") || lower.contains("13b") { 14.0 }
    else if lower.contains("7b") || lower.contains("8b") { 7.0 }
    else if lower.contains("3b") || lower.contains("4b") { 3.5 }
    else if lower.contains("1b") || lower.contains("2b") { 1.5 }
    else { 7.0 }
}

fn calculate_optimal_context(size_bytes: u64, name: &str) -> usize {
    let model_gb = if size_bytes > 0 {
        size_bytes as f64 / 1_000_000_000.0
    } else {
        estimate_params_from_name(name) * 0.55
    };
    
    // Tu hardware: RTX 2060 Super (8GB) + 64GB RAM
    if model_gb <= 4.0 { 524_288 }       // <4GB: 512k
    else if model_gb <= 7.0 { 393_216 }  // 4-7GB: 384k
    else if model_gb <= 10.0 { 262_144 } // 7-10GB: 256k
    else if model_gb <= 20.0 { 131_072 } // 10-20GB: 128k
    else if model_gb <= 40.0 { 65_536 }  // 20-40GB: 64k
    else { 32_768 }                       // >40GB: 32k
}

// ═══════════════════════════════════════════════════════════════════════════════
// GET ALL MODELS - DETECCIÓN AUTOMÁTICA DE APIs DISPONIBLES
// ═══════════════════════════════════════════════════════════════════════════════
fn get_all_models(gemini_key: Option<&str>, force_refresh: bool) -> Vec<DynamicModel> {
    // Si no forzamos refresh, intentar cache
    if !force_refresh {
        let cached = load_models_cache();
        if !cached.is_empty() {
            // Modelos locales siempre dinámicos (no de cache)
            let mut models: Vec<DynamicModel> = cached.into_iter()
                .filter(|m| !LOCAL_PROVIDERS.contains(&m.provider.as_str()))
                .collect();
            
            // Agregar modelos locales actuales
            models.extend(get_ollama_models());
            models.extend(get_all_local_api_models());
            
            return models;
        }
    }

    let mut all_models = Vec::new();
    
    log_info("🔍 Detectando APIs disponibles...");

    // ═══ GEMINI ═══
    let gemini_key_resolved = gemini_key.map(|s| s.to_string())
        .or_else(|| Some(get_api_key_from_sources("GEMINI_API_KEY", None)))
        .filter(|k| !k.is_empty());
    
    if let Some(ref key) = gemini_key_resolved {
        let gemini = fetch_gemini_models(key);
        if !gemini.is_empty() {
            log_ok(&format!("  Gemini: {} modelos", gemini.len()));
            all_models.extend(gemini);
        }
    }

    // ═══ GROQ ═══
    if !get_api_key_from_sources("GROQ_API_KEY", None).is_empty() {
        let groq = fetch_groq_models(None);
        if !groq.is_empty() {
            log_ok(&format!("  Groq: {} modelos", groq.len()));
            all_models.extend(groq);
        }
    }

    // ═══ DEEPSEEK ═══
    if !get_api_key_from_sources("DEEPSEEK_API_KEY", None).is_empty() {
        let deepseek = fetch_deepseek_models(None);
        if !deepseek.is_empty() {
            log_ok(&format!("  DeepSeek: {} modelos", deepseek.len()));
            all_models.extend(deepseek);
        }
    }

    // ═══ OPENROUTER ═══
    if !get_api_key_from_sources("OPENROUTER_API_KEY", None).is_empty() {
        let openrouter = fetch_openrouter_models();
        if !openrouter.is_empty() {
            log_ok(&format!("  OpenRouter: {} modelos", openrouter.len()));
            all_models.extend(openrouter);
        }
    }

    // ═══ CHUTES.AI ═══
    if !get_api_key_from_sources("CHUTES_API_KEY", None).is_empty() {
        let chutes = fetch_chutes_models(None);
        if !chutes.is_empty() {
            log_ok(&format!("  Chutes.ai: {} modelos", chutes.len()));
            all_models.extend(chutes);
        }
    }

    // ═══ SAMBANOVA ═══
    if !get_api_key_from_sources("SAMBANOVA_API_KEY", None).is_empty() {
        let sambanova = fetch_sambanova_models(None);
        if !sambanova.is_empty() {
            log_ok(&format!("  SambaNova: {} modelos", sambanova.len()));
            all_models.extend(sambanova);
        }
    }

    // ═══ CEREBRAS ═══
    if !get_api_key_from_sources("CEREBRAS_API_KEY", None).is_empty() {
        let cerebras = fetch_cerebras_models(None);
        if !cerebras.is_empty() {
            log_ok(&format!("  Cerebras: {} modelos", cerebras.len()));
            all_models.extend(cerebras);
        }
    }

    // ═══ TOGETHER ═══
    if !get_api_key_from_sources("TOGETHER_API_KEY", None).is_empty() {
        let together = fetch_together_models(None);
        if !together.is_empty() {
            log_ok(&format!("  Together: {} modelos", together.len()));
            all_models.extend(together);
        }
    }

    // ═══ FIREWORKS ═══
    if !get_api_key_from_sources("FIREWORKS_API_KEY", None).is_empty() {
        let fireworks = fetch_fireworks_models(None);
        if !fireworks.is_empty() {
            log_ok(&format!("  Fireworks: {} modelos", fireworks.len()));
            all_models.extend(fireworks);
        }
    }

    // ═══ COHERE ═══
    if !get_api_key_from_sources("COHERE_API_KEY", None).is_empty() {
        let cohere = fetch_cohere_models(None);
        if !cohere.is_empty() {
            log_ok(&format!("  Cohere: {} modelos", cohere.len()));
            all_models.extend(cohere);
        }
    }

    // ═══ HUGGING FACE ═══
    let hf_key = get_api_key_from_sources("HUGGINGFACE_API_KEY", None);
    let hf_token = get_api_key_from_sources("HF_TOKEN", None);
    if !hf_key.is_empty() || !hf_token.is_empty() {
        let hf = fetch_huggingface_models(None);
        if !hf.is_empty() {
            log_ok(&format!("  HuggingFace: {} modelos", hf.len()));
            all_models.extend(hf);
        }
    }

    // ═══ MISTRAL ═══
    if !get_api_key_from_sources("MISTRAL_API_KEY", None).is_empty() {
        let mistral = fetch_mistral_models(None);
        if !mistral.is_empty() {
            log_ok(&format!("  Mistral: {} modelos", mistral.len()));
            all_models.extend(mistral);
        }
    }

    // ═══ NOVITA ═══
    if !get_api_key_from_sources("NOVITA_API_KEY", None).is_empty() {
        let novita = fetch_novita_models(None);
        if !novita.is_empty() {
            log_ok(&format!("  Novita: {} modelos", novita.len()));
            all_models.extend(novita);
        }
    }

    // ═══ HYPERBOLIC ═══
    if !get_api_key_from_sources("HYPERBOLIC_API_KEY", None).is_empty() {
        let hyperbolic = fetch_hyperbolic_models(None);
        if !hyperbolic.is_empty() {
            log_ok(&format!("  HyperBolic: {} modelos", hyperbolic.len()));
            all_models.extend(hyperbolic);
        }
    }
    // ═══════════════════════════════════════════════════════════════
    // MODELOS LOCALES (siempre dinámicos, no de cache)
    // ═══════════════════════════════════════════════════════════════

    // ═══ OLLAMA ═══
    if is_ollama_running() {
        let ollama = get_ollama_models();
        if !ollama.is_empty() {
            log_ok(&format!("  Ollama (local): {} modelos", ollama.len()));
            all_models.extend(ollama);
        }
    }

    // ═══ APIS LOCALES CONFIGURADAS ═══
    let local_models = get_all_local_api_models();
    all_models.extend(local_models);

    // Guardar cache (solo modelos de APIs cloud, no locales)
    let cache_models: Vec<_> = all_models.iter()
        .filter(|m| !LOCAL_PROVIDERS.contains(&m.provider.as_str()))
        .cloned()
        .collect();
    if !cache_models.is_empty() {
        save_models_cache(&cache_models);
    }

    log_info(&format!("✓ Total: {} modelos disponibles", all_models.len()));
    all_models
}

fn refresh_all_models(gemini_key: Option<&str>) -> Vec<DynamicModel> {
    log_info("Actualizando lista de modelos desde APIs...");
    let models = get_all_models(gemini_key, true);
    
    // Mostrar resumen por proveedor
    let mut by_provider: HashMap<&str, usize> = HashMap::new();
    for m in &models {
        *by_provider.entry(m.provider.as_str()).or_insert(0) += 1;
    }
    
    for (provider, count) in &by_provider {
        log_ok(&format!("{}: {} modelos", provider.to_uppercase(), count));
    }
    
    log_ok(&format!("✓ Total: {} modelos disponibles", models.len()));
    models
}

/// Trunca display_name si es muy largo
fn truncate_display_name(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// GET ALTERNATIVE MODELS - DINÁMICO BASADO EN MODELOS DISPONIBLES
// ═══════════════════════════════════════════════════════════════════════════════

fn get_alternative_models(provider: &str, current: &str, all_models: &[DynamicModel]) -> Vec<String> {
    all_models
        .iter()
        .filter(|m| m.provider == provider && m.id != current)
        .map(|m| m.id.clone())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════════
// FIND MODEL - SIN ALIASES HARDCODEADOS
// ═══════════════════════════════════════════════════════════════════════════════

/// Busca un modelo por nombre - NO tiene fallback a Gemini
fn find_model(name: &str, models: &[DynamicModel]) -> Option<DynamicModel> {
    let name_lower = name.to_lowercase();
    
    // Búsqueda exacta primero
    if let Some(m) = models.iter().find(|m| m.name.to_lowercase() == name_lower) {
        return Some(m.clone());
    }
    
    // Búsqueda por ID exacto
    if let Some(m) = models.iter().find(|m| m.id.to_lowercase() == name_lower) {
        return Some(m.clone());
    }
    
    // Búsqueda parcial (contiene)
    let partial_matches: Vec<&DynamicModel> = models.iter()
        .filter(|m| {
            m.name.to_lowercase().contains(&name_lower) ||
            m.id.to_lowercase().contains(&name_lower) ||
            m.display_name.to_lowercase().contains(&name_lower)
        })
        .collect();
    
    match partial_matches.len() {
        0 => None,  // NO HAY FALLBACK - retorna None
        1 => Some(partial_matches[0].clone()),
        _ => {
            // Múltiples matches - mostrar TODOS y retornar None
            println!();
            log_warn(&format!("⚠️ '{}' coincide con {} modelos:", name, partial_matches.len()));
            println!();
            
            // Agrupar por proveedor para mejor visualización
            let mut by_provider: HashMap<&str, Vec<&DynamicModel>> = HashMap::new();
            for m in &partial_matches {
                by_provider.entry(m.provider.as_str()).or_default().push(m);
            }
            
            for (provider, provider_models) in by_provider.iter() {
                println!("  {}:", provider.to_uppercase().cyan().bold());
                for m in provider_models {
                    println!("    {} - {}", 
                        m.name.green(), 
                        m.display_name,
                    );
                }
            }
            
            println!();
            log_info("Especifica el nombre exacto del modelo");
            
            None  // NO SELECCIONAR AUTOMÁTICAMENTE
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// OLLAMA ENV VARS - CONFIGURACIÓN DINÁMICA
// ═══════════════════════════════════════════════════════════════════════════════

fn get_ollama_env_vars(model_name: &str) -> Vec<(String, String)> {
    let models = get_ollama_models();
    
    let model_info = models.iter().find(|m| 
        m.name == model_name || 
        m.name == model_name.replace(":latest", "") ||
        model_name.contains(&m.name)
    );
    
    let num_ctx = model_info.map(|m| m.token_limit).unwrap_or(131_072);
    
    vec![
        ("OLLAMA_NUM_CTX".to_string(), num_ctx.to_string()),
        ("OLLAMA_NUM_GPU".to_string(), "99".to_string()),
        ("OLLAMA_NUM_THREAD".to_string(), "16".to_string()),
        ("OLLAMA_FLASH_ATTENTION".to_string(), "1".to_string()),
        ("OLLAMA_MMAP".to_string(), "1".to_string()),
        ("OLLAMA_NUM_PARALLEL".to_string(), if num_ctx > 262_144 { "1" } else { "2" }.to_string()),
    ]
}


// ═══════════════════════════════════════════════════════════════════════════════
// OLLAMA OPTIMIZADO PARA RTX 2060 SUPER (8GB) + 64GB RAM + i7-13700K
// ═══════════════════════════════════════════════════════════════════════════════

fn get_ollama_env_vars_optimized(_model_name: &str) -> Vec<(String, String)> {
    let config = load_ollama_config();
    
    // Mostrar config activa
    if let Some(ctx) = config.get("OLLAMA_NUM_CTX") {
        let ctx_k: usize = ctx.parse().unwrap_or(32768) / 1024;
        log_info(&format!("   Contexto: {}k tokens", ctx_k));
    }
    if let Some(gpu) = config.get("OLLAMA_NUM_GPU") {
        log_info(&format!("   GPU layers: {}", gpu));
    }
    
    config.into_iter().collect()
}

/// Reinicia Ollama con configuración del archivo externo
fn restart_ollama_with_config(model_name: &str) {
    log_info("🔄 Reiniciando Ollama con configuración externa...");
    
    // Matar proceso existente
    let _ = Command::new("pkill").args(["-9", "ollama"]).output();
    thread::sleep(Duration::from_secs(2));
    
    // Cargar configuración
    let config = load_ollama_config();
    
    // Mostrar resumen
    println!();
    log_info("═══ CONFIGURACIÓN OLLAMA ═══");
    if let Some(ctx) = config.get("OLLAMA_NUM_CTX") {
        let ctx_val: usize = ctx.parse().unwrap_or(0);
        log_info(&format!("  Contexto: {} tokens ({}k)", ctx_val, ctx_val / 1024));
    }
    if let Some(gpu) = config.get("OLLAMA_NUM_GPU") {
        log_info(&format!("  GPU layers: {}", gpu));
    }
    if let Some(fa) = config.get("OLLAMA_FLASH_ATTENTION") {
        log_info(&format!("  Flash Attention: {}", if fa == "1" { "ON" } else { "OFF" }));
    }
    println!();
    
    // Iniciar Ollama con config
    let mut cmd = Command::new("ollama");
    cmd.arg("serve");
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    
    for (key, value) in &config {
        cmd.env(key, value);
    }
    
    match cmd.spawn() {
        Ok(_) => {
            // Esperar a que esté listo
            for i in 0..60 {
                thread::sleep(Duration::from_millis(500));
                if is_ollama_running() {
                    log_ok(&format!("✓ Ollama listo después de {}s", (i + 1) / 2));
                    
                    // Pre-cargar modelo
                    if !model_name.is_empty() && model_name != "default" {
                        log_info(&format!("Pre-cargando modelo {}...", model_name));
                        let _ = Command::new("ollama")
                            .args(["run", model_name, "--keepalive", "24h"])
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .spawn();
                    }
                    return;
                }
            }
            log_warn("Ollama tardando en iniciar...");
        }
        Err(e) => log_error(&format!("Error iniciando Ollama: {}", e)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// LECTURA CON TIMEOUT - SOLUCIÓN AL BLOQUEO
// ═══════════════════════════════════════════════════════════════════════════════
/// Lee líneas con timeout - retorna un receiver
fn read_lines_with_timeout<R: std::io::Read + Send + 'static>(
    reader: R,
    _timeout_per_line: Duration,  // Se usa en recv_timeout, no aquí
) -> Receiver<Option<String>> {
    let (tx, rx) = mpsc::channel();
    
    thread::spawn(move || {
        let buf_reader = BufReader::new(reader);
        for line in buf_reader.lines() {
            match line {
                Ok(l) => {
                    if tx.send(Some(l)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = tx.send(None);
    });
    
    rx
}
/// Configuración optimizada de Ollama para máximo contexto con GPU + RAM offload
fn get_ollama_env_vars_optimizedold(model_name: &str) -> Vec<(String, String)> {
    // Tu hardware:
    // - RTX 2060 Super: 8GB VRAM
    // - RAM: 64GB DDR4/5
    // - CPU: i7-13700K (8P + 8E cores = 24 threads)
    
    // Estrategia para máximo contexto:
    // 1. Cargar modelo en VRAM (hasta 7GB)
    // 2. KV cache overflow a RAM (hasta 50GB disponibles)
    // 3. Flash attention para eficiencia
    // 4. Usar mmap para no duplicar modelo en RAM
    
    let model_size_gb = estimate_ollama_model_size(model_name);
    
    // Calcular capas que caben en GPU
    // RTX 2060S tiene 8GB, dejamos 1GB para overhead de CUDA
    let available_vram_gb = 7.0;
    let layers_in_vram = if model_size_gb <= available_vram_gb {
        99  // Todo en GPU
    } else {
        // Calcular cuántas capas caben
        let ratio = available_vram_gb / model_size_gb;
        (ratio * 35.0) as i32  // Asumiendo ~35 capas promedio
    };
    
    // Calcular contexto máximo
    // KV cache: ~2 bytes por token por capa para FP16, ~1 byte para Q8
    // Con 64GB RAM disponibles y modelo en GPU, podemos tener contexto masivo
    let max_context = calculate_max_context_for_hardware(model_size_gb);
    
    vec![
        // Contexto máximo aprovechando RAM
        ("OLLAMA_NUM_CTX".to_string(), max_context.to_string()),
        
        // Capas en GPU (todas las posibles)
        ("OLLAMA_NUM_GPU".to_string(), layers_in_vram.to_string()),
        
        // Threads para CPU (usar P-cores principalmente)
        ("OLLAMA_NUM_THREAD".to_string(), "16".to_string()),
        
        // Flash Attention (reduce memoria, más rápido)
        ("OLLAMA_FLASH_ATTENTION".to_string(), "1".to_string()),
        
        // Usar mmap para cargar modelo sin duplicar en RAM
        ("OLLAMA_MMAP".to_string(), "1".to_string()),
        
        // Mantener modelo en memoria entre requests
        ("OLLAMA_KEEP_ALIVE".to_string(), "24h".to_string()),
        
        // Batch size para throughput
        ("OLLAMA_NUM_BATCH".to_string(), "512".to_string()),
        
        // Paralelismo (reducir para más contexto)
        ("OLLAMA_NUM_PARALLEL".to_string(), "1".to_string()),
        
        // CUDA optimizations
        ("CUDA_VISIBLE_DEVICES".to_string(), "0".to_string()),
        
        // Desactivar swap para mejor performance
        ("OLLAMA_NOSWAP".to_string(), "1".to_string()),
    ]
}


/// Estima tamaño del modelo en GB por nombre
fn estimate_ollama_model_size(name: &str) -> f64 {
    let lower = name.to_lowercase();
    
    // Por parámetros en el nombre
    if lower.contains("70b") || lower.contains("72b") { 40.0 }      // Q4
    else if lower.contains("34b") || lower.contains("32b") { 19.0 }
    else if lower.contains("14b") || lower.contains("13b") { 8.0 }
    else if lower.contains("8b") { 4.5 }
    else if lower.contains("7b") { 4.0 }
    else if lower.contains("3b") || lower.contains("4b") { 2.5 }
    else if lower.contains("1b") || lower.contains("2b") { 1.5 }
    else { 4.0 }  // Default 7B size
}

/// Calcula contexto máximo para tu hardware específico
fn calculate_max_context_for_hardware(model_size_gb: f64) -> usize {
    // Hardware: 64GB RAM, 8GB VRAM
    // Asumimos:
    // - Modelo en VRAM (o parcialmente)
    // - KV cache principalmente en RAM
    // - Sistema usa ~8GB RAM
    // - Disponible para KV cache: ~50GB
    
    let available_ram_gb = 50.0;
    
    // KV cache size estimation:
    // Para Llama-style: 2 * num_layers * hidden_dim * 2 (K+V) * 2 bytes (FP16)
    // Simplificado: ~0.5MB por 1k tokens para modelo 7B
    // Para modelos más grandes, escala linealmente
    
    let mb_per_1k_tokens = if model_size_gb <= 4.0 { 0.4 }
        else if model_size_gb <= 8.0 { 0.6 }
        else if model_size_gb <= 20.0 { 1.0 }
        else { 2.0 };
    
    let max_tokens = ((available_ram_gb * 1024.0) / mb_per_1k_tokens * 1000.0) as usize;
    
    // Cap razonable y redondear a número bonito
    let capped = max_tokens.min(1_048_576);  // Max 1M
    
    // Redondear a potencia de 2 más cercana
    let powers = [32_768, 65_536, 131_072, 262_144, 524_288, 786_432, 1_048_576];
    powers.iter().copied().filter(|&p| p <= capped).last().unwrap_or(131_072)
}

fn fetch_ollama_models_api() -> Option<Vec<DynamicModel>> {
    let output = Command::new("curl")
        .args(["-s", "--connect-timeout", "3", "http://localhost:11434/api/tags"])
        .output()
        .ok()?;
    
    if !output.status.success() {
        return None;
    }
    
    let json = String::from_utf8_lossy(&output.stdout);
    let mut models = Vec::new();
    let mut current_name = String::new();
    let mut current_size: u64 = 0;
    
    for line in json.split(&['{', '}', ','][..]) {
        let line = line.trim();
        
        if line.contains("\"name\"") {
            if let Some(start) = line.find(':') {
                let value = line[start + 1..].trim().trim_matches('"');
                let clean = value.replace("\"", "").trim().to_string();
                if !clean.is_empty() && !clean.starts_with(':') {
                    current_name = clean;
                }
            }
        }
        
        if line.contains("\"size\"") {
            if let Some(start) = line.find(':') {
                let value = line[start + 1..].trim();
                let num_str: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
                current_size = num_str.parse().unwrap_or(0);
            }
        }
        
        if !current_name.is_empty() && (current_size > 0 || line.contains("modified")) {
            models.push(create_ollama_model_entry(&current_name, current_size));
            current_name.clear();
            current_size = 0;
        }
    }
    
    if !current_name.is_empty() {
        models.push(create_ollama_model_entry(&current_name, 0));
    }
    
    Some(models)
}

fn fetch_ollama_models_cli() -> Vec<DynamicModel> {
    let output = match Command::new("ollama").arg("list").output() {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() { return None; }
            
            let name = parts[0].to_string();
            let mut size_bytes: u64 = 0;
            
            for (i, part) in parts.iter().enumerate() {
                if *part == "GB" && i > 0 {
                    if let Ok(n) = parts[i-1].parse::<f64>() {
                        size_bytes = (n * 1e9) as u64;
                    }
                } else if *part == "MB" && i > 0 {
                    if let Ok(n) = parts[i-1].parse::<f64>() {
                        size_bytes = (n * 1e6) as u64;
                    }
                } else if part.ends_with("GB") {
                    if let Ok(n) = part.trim_end_matches("GB").parse::<f64>() {
                        size_bytes = (n * 1e9) as u64;
                    }
                }
            }
            
            Some(create_ollama_model_entry(&name, size_bytes))
        })
        .collect()
}

fn create_ollama_model_entry(name: &str, size_bytes: u64) -> DynamicModel {
    let short_name = name.replace(":latest", "");
    let model_size_gb = if size_bytes > 0 { 
        size_bytes as f64 / 1e9 
    } else { 
        estimate_ollama_model_size(&short_name) 
    };
    
    let max_context = calculate_max_context_for_hardware(model_size_gb);
    
    let display = format!("{} ({:.1}GB, {}k ctx, local)", 
        short_name.replace("-", " ").replace("_", " "),
        model_size_gb,
        max_context / 1000
    );
    
    DynamicModel {
        name: short_name,
        display_name: display,
        provider: "ollama".to_string(),
        token_limit: max_context,
        is_free: true,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// DYNAMIC MODE - MODELO ELIGE ARCHIVOS VIA TREE
// ═══════════════════════════════════════════════════════════════════════════════

/// Genera tree del proyecto para que el modelo elija archivos
fn generate_project_tree(workspace: &Path) -> String {
    // Intentar usar tree si está disponible
    if let Ok(output) = Command::new("tree")
        .args(["-I", "target|node_modules|.git|.aider*|*.orig|__pycache__", "-L", "4", "--noreport"])
        .current_dir(workspace)
        .output() 
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).to_string();
        }
    }
    
    // Fallback: generar tree manualmente
    generate_tree_manual(workspace, "", 0, 4)
}

fn generate_tree_manual(dir: &Path, prefix: &str, depth: usize, max_depth: usize) -> String {
    if depth >= max_depth {
        return String::new();
    }
    
    let mut result = String::new();
    let ignored = ["target", "node_modules", ".git", ".aider", "__pycache__", ".venv"];
    
    let mut entries: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            !ignored.iter().any(|i| name.starts_with(i))
        })
        .collect();
    
    entries.sort_by_key(|e| (!e.path().is_dir(), e.file_name()));
    
    for (i, entry) in entries.iter().enumerate() {
        let is_last = i == entries.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let name = entry.file_name().to_string_lossy().to_string();
        
        result.push_str(&format!("{}{}{}\n", prefix, connector, name));
        
        if entry.path().is_dir() {
            let new_prefix = if is_last {
                format!("{}    ", prefix)
            } else {
                format!("{}│   ", prefix)
            };
            result.push_str(&generate_tree_manual(&entry.path(), &new_prefix, depth + 1, max_depth));
        }
    }
    
    result
}

/// Prompt para que el modelo elija archivos
fn create_file_selection_prompt(workspace: &Path, task: &str) -> String {
    let tree = generate_project_tree(workspace);
    
    format!(r#"You are about to work on this task:
{}

Here is the project structure:
{}

RESPOND WITH ONLY a JSON array of file paths you need to read/modify.
Example: ["src/main.rs", "src/lib.rs", "Cargo.toml"]

Choose the MINIMUM files needed. Max 15 files.
Only include files that exist and are relevant to the task.
DO NOT include any explanation, ONLY the JSON array."#, task, tree)
}

/// Selecciona archivos dinámicamente - el modelo elige
fn select_files_dynamic(state: &mut AppState, task: &str, max_files: usize) -> Vec<PathBuf> {
    log_info(&format!("🌳 Modo dinámico: máximo {} archivos", max_files));
    
    // Generar tree
    let tree = generate_project_tree(&state.workspace);
    
    // Mostrar tree al usuario
    println!();
    println!("{}", "📂 Estructura del proyecto:".cyan().bold());
    println!("{}", "─".repeat(60));
    for (i, line) in tree.lines().enumerate() {
        if i < 40 {
            println!("{}", line.dimmed());
        } else if i == 40 {
            println!("{}", "  ... (más archivos)".dimmed());
            break;
        }
    }
    println!("{}", "─".repeat(60));
    println!();
    
    // Para Ollama, NO usar aider para selección (muy lento)
    // Usar heurística basada en la tarea
    if state.model.provider == "ollama" {
        log_info("🦙 Ollama: usando selección heurística (más rápido)...");
        return select_files_heuristic(&state.workspace, task, max_files);
    }
    
    log_info("🤖 Consultando al modelo qué archivos necesita...");
    
    let prompt = format!(r#"Analyze this task and select which files to load.

TASK: {}

PROJECT FILES:
{}

RULES:
- Select ONLY files needed for this task
- Maximum {} files
- Output ONLY a JSON array of file paths
- Example: ["src/main.rs", "Cargo.toml"]

Your selection (JSON array only):"#, task, tree, max_files);

    // Usar curl directamente para APIs cloud (más rápido que aider para esto)
    let selected = if state.model.provider == "gemini" {
        query_gemini_for_files(&prompt, state)
    } else {
        query_aider_for_files(&prompt, state)
    };
    
    if selected.is_empty() {
        log_warn("No se obtuvieron archivos, usando selección automática");
        return select_files(&state.workspace, state.model.token_limit);
    }
    
    // Validar y limitar
    let valid_files: Vec<PathBuf> = selected.iter()
        .take(max_files)
        .filter_map(|f| {
            let path = state.workspace.join(f);
            if path.exists() { Some(path) } else { None }
        })
        .collect();
    
    if valid_files.is_empty() {
        log_warn("Archivos seleccionados no existen, usando automático");
        return select_files(&state.workspace, state.model.token_limit);
    }
    
    log_ok(&format!("📂 Seleccionados {} archivos:", valid_files.len()));
    for f in &valid_files {
        let rel = f.strip_prefix(&state.workspace).unwrap_or(f);
        println!("    {}", rel.display().to_string().green());
    }
    
    valid_files
}

/// Selección heurística basada en palabras clave de la tarea
fn select_files_heuristic(workspace: &Path, task: &str, max_files: usize) -> Vec<PathBuf> {
    let task_lower = task.to_lowercase();
    let all_files = get_source_files(workspace);
    
    let mut scored_files: Vec<(PathBuf, i32)> = all_files.iter()
        .map(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            let rel_path = path.strip_prefix(workspace).unwrap_or(path).to_string_lossy().to_lowercase();
            let mut score = 0i32;
            
            // Archivos principales siempre importantes
            if name == "main.rs" || name == "lib.rs" { score += 100; }
            if name == "cargo.toml" || name == "package.json" { score += 50; }
            if name == "mod.rs" { score += 30; }
            
            // Buscar coincidencias con la tarea
            let keywords: Vec<&str> = task_lower.split_whitespace()
                .filter(|w| w.len() > 3)
                .collect();
            
            for keyword in &keywords {
                if name.contains(keyword) { score += 50; }
                if rel_path.contains(keyword) { score += 25; }
            }
            
            // Palabras clave comunes
            if task_lower.contains("error") || task_lower.contains("fix") || task_lower.contains("bug") {
                // Para errores, preferir archivos mencionados o principales
                if name.contains("error") || name.contains("handler") { score += 40; }
            }
            if task_lower.contains("test") {
                if name.contains("test") { score += 60; }
            }
            if task_lower.contains("api") || task_lower.contains("http") {
                if rel_path.contains("api") || name.contains("client") { score += 40; }
            }
            if task_lower.contains("database") || task_lower.contains("db") {
                if rel_path.contains("db") || name.contains("model") { score += 40; }
            }
            
            (path.clone(), score)
        })
        .collect();
    
    // Ordenar por score descendente
    scored_files.sort_by(|a, b| b.1.cmp(&a.1));
    
    // Tomar los mejores hasta el límite
    scored_files.into_iter()
        .take(max_files)
        .map(|(path, _)| path)
        .collect()
}

/// Consulta a Gemini directamente para selección de archivos
fn query_gemini_for_files(prompt: &str, state: &AppState) -> Vec<String> {
    let key = match state.current_key() {
        Some(k) => k,
        None => return Vec::new(),
    };
    
    let body = format!(r#"{{"contents":[{{"parts":[{{"text":"{}"}}]}}]}}"#, 
        prompt.replace('"', "\\\"").replace('\n', "\\n"));
    
    let output = Command::new("curl")
        .args([
            "-s", "--connect-timeout", "30",
            "-H", "Content-Type: application/json",
            "-d", &body,
            &format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent?key={}", key)
        ])
        .output();
    
    match output {
        Ok(o) if o.status.success() => {
            let response = String::from_utf8_lossy(&o.stdout);
            parse_file_selection(&response)
        }
        _ => Vec::new()
    }
}

/// Usa aider para consultar archivos (fallback)
fn query_aider_for_files(prompt: &str, state: &mut AppState) -> Vec<String> {
    let mut cmd = Command::new("aider");
    cmd.current_dir(&state.workspace)
        .args([
            "--model", &state.model.id,
            "--no-auto-commits",
            "--yes-always",
            "--no-stream",
            "--message", prompt,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("BROWSER", "/bin/true");
    
    if state.model.provider == "ollama" {
        for (k, v) in get_ollama_env_vars_optimized(&state.model.name) {
            cmd.env(&k, &v);
        }
    }

    match cmd.output() {
        Ok(o) if o.status.success() => {
            parse_file_selection(&String::from_utf8_lossy(&o.stdout))
        }
        _ => Vec::new()
    }
}

/// Parsea la respuesta del modelo buscando array JSON
fn parse_file_selection(response: &str) -> Vec<String> {
    // Buscar array JSON en cualquier parte de la respuesta
    let mut files = Vec::new();
    
    // Método 1: Buscar [...] directamente
    if let Some(start) = response.find('[') {
        if let Some(end) = response[start..].find(']') {
            let json_str = &response[start..start + end + 1];
            
            // Parsear manualmente el array
            let mut current = String::new();
            let mut in_string = false;
            let mut escape_next = false;
            
            for c in json_str.chars() {
                if escape_next {
                    current.push(c);
                    escape_next = false;
                    continue;
                }
                
                match c {
                    '\\' => escape_next = true,
                    '"' => {
                        if in_string {
                            // Fin de string
                            let trimmed = current.trim().to_string();
                            if !trimmed.is_empty() && 
                               (trimmed.contains('.') || trimmed.contains('/')) {
                                files.push(trimmed);
                            }
                            current.clear();
                        }
                        in_string = !in_string;
                    }
                    _ if in_string => current.push(c),
                    _ => {}
                }
            }
        }
    }
    
    // Método 2: Si no encontró JSON, buscar líneas que parezcan paths
    if files.is_empty() {
        for line in response.lines() {
            let trimmed = line.trim()
                .trim_matches(|c| c == '"' || c == '\'' || c == ',' || c == '[' || c == ']');
            
            if trimmed.contains('/') && 
               (trimmed.ends_with(".rs") || trimmed.ends_with(".py") || 
                trimmed.ends_with(".toml") || trimmed.ends_with(".js") ||
                trimmed.ends_with(".ts") || trimmed.ends_with(".go")) {
                files.push(trimmed.to_string());
            }
        }
    }
    
    files
}

// ═══════════════════════════════════════════════════════════════════════════════
// STRING UTILITIES - UTF-8 SAFE
// ═══════════════════════════════════════════════════════════════════════════════

/// Salta el primer carácter de forma segura (para líneas de diff que empiezan con +, -, o espacio)
fn skip_first_char(s: &str) -> &str {
    if s.is_empty() {
        return "";
    }
    // Los caracteres +, -, y espacio son ASCII (1 byte), así que [1..] es seguro aquí
    // Pero para ser 100% seguros con cualquier UTF-8:
    s.char_indices()
        .nth(1)
        .map(|(idx, _)| &s[idx..])
        .unwrap_or("")
}

/// Salta los primeros N caracteres de forma segura
fn skip_n_chars(s: &str, n: usize) -> &str {
    s.char_indices()
        .nth(n)
        .map(|(idx, _)| &s[idx..])
        .unwrap_or("")
}

/// Toma los primeros N caracteres de forma segura
fn take_n_chars(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Une los primeros N elementos de un Vec<String>
fn join_first_n(items: &[String], n: usize, separator: &str) -> String {
    items.iter()
        .take(n)
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(separator)
}

/// Configura la API key según el proveedor
fn configure_api_key_for_provider(cmd: &mut Command, provider: &str, key: &str) {
    match provider.to_lowercase().as_str() {
        "gemini" => { 
            cmd.env("GEMINI_API_KEY", key); 
        }
        "groq" => { 
            cmd.env("GROQ_API_KEY", key); 
        }
        "openrouter" => { 
            cmd.env("OPENROUTER_API_KEY", key); 
        }
        "deepseek" => { 
            cmd.env("DEEPSEEK_API_KEY", key); 
        }
        "together" => { 
            cmd.env("TOGETHER_API_KEY", key); 
        }
        "fireworks" => { 
            cmd.env("FIREWORKS_API_KEY", key); 
        }
        "anthropic" => { 
            cmd.env("ANTHROPIC_API_KEY", key); 
        }
        "openai" => { 
            cmd.env("OPENAI_API_KEY", key); 
        }
        "chutes" => {
            cmd.env("CHUTES_API_KEY", key);
        }
        "sambanova" => {
            cmd.env("SAMBANOVA_API_KEY", key);
        }
        "cerebras" => {
            cmd.env("CEREBRAS_API_KEY", key);
        }
        "cohere" => {
            cmd.env("COHERE_API_KEY", key);
        }
        "huggingface" => {
            cmd.env("HUGGINGFACE_API_KEY", key);
            cmd.env("HF_TOKEN", key);
        }
        "mistral" => {
            cmd.env("MISTRAL_API_KEY", key);
        }
        "novita" => {
            cmd.env("NOVITA_API_KEY", key);
        }
        "hyperbolic" => {
            cmd.env("HYPERBOLIC_API_KEY", key);
        }
        _ => { 
            // Genérico - intentar OpenAI
            cmd.env("OPENAI_API_KEY", key); 
        }
    }
}

/// Procesa una línea de output (extraído para evitar duplicación)
fn process_output_line(
    line: &str,
    line_count: &mut usize,
    response_started: &mut bool,
    full_output: &mut String,
    current_file: &mut Option<String>,
    in_search: &mut bool,
    in_replace: &mut bool,
    search_content: &mut String,
    replace_content: &mut String,
    aider_modified_files: &mut Vec<String>,
    state: &mut AppState,
) -> Option<ApiError> {
    *line_count += 1;
    
    if !*response_started && *line_count == 1 {
        log_ok("📨 Respuesta recibida - procesando...");
        *response_started = true;
    }

    // Mostrar output
    if *line_count <= 500 || line.contains("Applied edit") || line.contains("SEARCH") 
       || line.contains("REPLACE") || line.contains("error") {
        println!("{}", line);
    } else if *line_count == 501 {
        println!("{}", "... (output continúa)".dimmed());
    }
    
    // Acumular output (con límite)
    if full_output.len() < 10_000_000 {
        full_output.push_str(&format!("{}\n", line));
    }

    let trimmed = line.trim();
    
    // Detectar nombre de archivo
    if is_filename_line(trimmed) && !*in_search && !*in_replace {
        if let Some(ref file) = current_file {
            if !search_content.is_empty() {
                let edit = PendingEdit {
                    filename: file.clone(),
                    search: search_content.clone(),
                    replace: replace_content.clone(),
                };
                state.buffer_pending_edit(edit);
            }
        }
        *current_file = Some(trimmed.to_string());
        search_content.clear();
        replace_content.clear();
    }
    
    if line.contains("<<<<<<< SEARCH") {
        *in_search = true;
        *in_replace = false;
        search_content.clear();
    } else if line.contains("=======") && *in_search {
        *in_search = false;
        *in_replace = true;
        replace_content.clear();
    } else if line.contains(">>>>>>> REPLACE") {
        if let Some(ref file) = current_file {
            if search_content.len() <= MAX_SEARCH_REPLACE_BYTES && 
               replace_content.len() <= MAX_SEARCH_REPLACE_BYTES {
                let edit = PendingEdit {
                    filename: file.clone(),
                    search: search_content.clone(),
                    replace: replace_content.clone(),
                };
                
                if apply_single_edit_safely(&state.workspace, &edit, state.verbose) {
                    if !aider_modified_files.contains(file) {
                        aider_modified_files.push(file.clone());
                    }
                    log_ok(&format!("💾 Aplicado: {}", file));
                } else {
                    state.buffer_pending_edit(edit);
                }
            }
        }
        *in_search = false;
        *in_replace = false;
        search_content.clear();
        replace_content.clear();
    } else if *in_search {
        if search_content.len() < MAX_SEARCH_REPLACE_BYTES {
            search_content.push_str(line);
            search_content.push('\n');
        }
    } else if *in_replace {
        if replace_content.len() < MAX_SEARCH_REPLACE_BYTES {
            replace_content.push_str(line);
            replace_content.push('\n');
        }
    }

    // Detectar archivos aplicados por aider
    if line.contains("Applied edit to") {
        if let Some(f) = line.split("Applied edit to").nth(1) {
            let file = f.trim().to_string();
            if !aider_modified_files.contains(&file) && !file.is_empty() {
                aider_modified_files.push(file.clone());
            }
        }
    }

    // Detectar errores de API
    let error = detect_api_error(line);
    if error != ApiError::None {
        return Some(error);
    }
    
    None
}

fn process_single_diff(
    workspace: &Path,
    filename: &str,
    search: &str,
    replace: &str,
    verbose: bool,
) -> DiffResult {
    // Verificar si tiene código lazy
    if contains_lazy_pattern(replace) {
        log_warn(&format!("Rechazado {} por código lazy", filename));
        return DiffResult::Rejected;
    }
    
    let path = workspace.join(filename);
    
    if !path.exists() {
        // Si es archivo nuevo (search vacío), crear
        if search.trim().is_empty() && !replace.trim().is_empty() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            match fs::write(&path, replace.trim()) {
                Ok(_) => {
                    log_ok(&format!("✓ Creado: {}", filename));
                    return DiffResult::Applied;
                }
                Err(e) => {
                    log_error(&format!("Error creando {}: {}", filename, e));
                    return DiffResult::Skipped;
                }
            }
        }
        log_debug(&format!("Archivo no existe: {}", filename), verbose);
        return DiffResult::Skipped;
    }
    
    let original = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            log_debug(&format!("Error leyendo {}: {}", filename, e), verbose);
            return DiffResult::Skipped;
        }
    };
    
    let search_trimmed = search.trim();
    
    // Buscar el patrón
    if !original.contains(search_trimmed) {
        // Intentar fuzzy match
        if let Some(matched) = find_fuzzy_match(&original, search_trimmed) {
            let new_content = original.replace(&matched, replace.trim());
            
            if let Err(e) = validate_change_safety(&original, &new_content, filename) {
                log_warn(&format!("Cambio rechazado en {}: {}", filename, e));
                return DiffResult::Skipped;
            }
            
            match fs::write(&path, &new_content) {
                Ok(_) => {
                    log_ok(&format!("✓ Aplicado (fuzzy): {}", filename));
                    return DiffResult::Applied;
                }
                Err(e) => {
                    log_error(&format!("Error escribiendo {}: {}", filename, e));
                    return DiffResult::Skipped;
                }
            }
        }
        log_debug(&format!("SEARCH no encontrado en {}", filename), verbose);
        return DiffResult::Skipped;
    }
    
    let new_content = original.replace(search_trimmed, replace.trim());
    
    if let Err(e) = validate_change_safety(&original, &new_content, filename) {
        log_warn(&format!("Cambio rechazado en {}: {}", filename, e));
        return DiffResult::Skipped;
    }
    
    match fs::write(&path, &new_content) {
        Ok(_) => {
            log_ok(&format!("✓ Aplicado: {}", filename));
            DiffResult::Applied
        }
        Err(e) => {
            log_error(&format!("Error escribiendo {}: {}", filename, e));
            DiffResult::Skipped
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// PROCESS_DIFFS_SAFELY_WITH_REJECTED - IMPLEMENTACIÓN COMPLETA
// ═══════════════════════════════════════════════════════════════════════════════

/// Procesa diffs del output y retorna (modificados, rechazados)
fn process_diffs_safely_with_rejected(
    workspace: &Path, 
    output: &str, 
    verbose: bool
) -> (Vec<String>, Vec<String>) {
    let mut modified_files = Vec::new();
    let mut rejected_files = Vec::new();
    
    let mut current_file: Option<String> = None;
    let mut in_search = false;
    let mut in_replace = false;
    let mut search_content = String::new();
    let mut replace_content = String::new();
    
    for line in output.lines() {
        let trimmed = line.trim();
        
        // Detectar nombre de archivo
        if is_filename_line(trimmed) && !in_search && !in_replace {
            // Procesar edit anterior si existe
            if let Some(ref file) = current_file {
                if !search_content.is_empty() || !replace_content.is_empty() {
                    let result = process_single_diff(
                        workspace, file, &search_content, &replace_content, verbose
                    );
                    match result {
                        DiffResult::Applied => {
                            if !modified_files.contains(file) {
                                modified_files.push(file.clone());
                            }
                        }
                        DiffResult::Rejected => {
                            if !rejected_files.contains(file) {
                                rejected_files.push(file.clone());
                            }
                        }
                        DiffResult::Skipped => {}
                    }
                }
            }
            current_file = Some(trimmed.to_string());
            search_content.clear();
            replace_content.clear();
        }
        
        if line.contains("<<<<<<< SEARCH") {
            in_search = true;
            in_replace = false;
            search_content.clear();
        } else if line.contains("=======") && in_search {
            in_search = false;
            in_replace = true;
            replace_content.clear();
        } else if line.contains(">>>>>>> REPLACE") {
            // Procesar este bloque
            if let Some(ref file) = current_file {
                // Verificar límite de tamaño
                if search_content.len() > MAX_SEARCH_REPLACE_BYTES || 
                   replace_content.len() > MAX_SEARCH_REPLACE_BYTES {
                    log_warn(&format!("Bloque demasiado grande en {} - rechazado", file));
                    if !rejected_files.contains(file) {
                        rejected_files.push(file.clone());
                    }
                } else {
                    let result = process_single_diff(
                        workspace, file, &search_content, &replace_content, verbose
                    );
                    match result {
                        DiffResult::Applied => {
                            if !modified_files.contains(file) {
                                modified_files.push(file.clone());
                            }
                        }
                        DiffResult::Rejected => {
                            if !rejected_files.contains(file) {
                                rejected_files.push(file.clone());
                            }
                        }
                        DiffResult::Skipped => {}
                    }
                }
            }
            in_search = false;
            in_replace = false;
            search_content.clear();
            replace_content.clear();
        } else if in_search {
            if search_content.len() < MAX_SEARCH_REPLACE_BYTES {
                search_content.push_str(line);
                search_content.push('\n');
            }
        } else if in_replace {
            if replace_content.len() < MAX_SEARCH_REPLACE_BYTES {
                replace_content.push_str(line);
                replace_content.push('\n');
            }
        }
    }
    
    // Procesar último bloque si existe
    if let Some(ref file) = current_file {
        if !search_content.is_empty() || !replace_content.is_empty() {
            let result = process_single_diff(
                workspace, file, &search_content, &replace_content, verbose
            );
            match result {
                DiffResult::Applied => {
                    if !modified_files.contains(file) {
                        modified_files.push(file.clone());
                    }
                }
                DiffResult::Rejected => {
                    if !rejected_files.contains(file) {
                        rejected_files.push(file.clone());
                    }
                }
                DiffResult::Skipped => {}
            }
        }
    }
    
    (modified_files, rejected_files)
}

/// Trunca un string de forma segura respetando límites de caracteres UTF-8
// ═══════════════════════════════════════════════════════════════════════════════
// STRING UTILITIES - UTF-8 SAFE
// ═══════════════════════════════════════════════════════════════════════════════

/// Trunca un string de forma segura respetando límites UTF-8
fn truncate_safe(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}

/// Trunca sin agregar "..."
fn truncate_exact(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// Enmascara una API key de forma segura
fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let len = chars.len();
    
    if len < 12 {
        return "***".to_string();
    }
    
    let prefix_len = 8.min(len / 3);
    let suffix_len = 4.min(len / 4);
    
    let prefix: String = chars.iter().take(prefix_len).collect();
    let suffix: String = chars.iter().skip(len.saturating_sub(suffix_len)).collect();
    
    format!("{}...{}", prefix, suffix)
}

/// Obtiene los primeros N caracteres de forma segura
fn first_n_chars(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Obtiene los últimos N caracteres de forma segura
fn last_n_chars(s: &str, n: usize) -> &str {
    let char_count = s.chars().count();
    if char_count <= n {
        return s;
    }
    match s.char_indices().nth(char_count - n) {
        Some((idx, _)) => &s[idx..],
        None => s,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ACTUALIZAR run_autonomous PARA USAR DYNAMIC MODE
// ═══════════════════════════════════════════════════════════════════════════════
// run_autonomous también debe pasar el timeout
fn run_autonomous(
    state: &mut AppState, 
    initial_task: &str, 
    dynamic_mode: bool, 
    max_files: usize,
    timeout_minutes: u64,  
) {
       let base_task = initial_task.to_string();
    ensure_git_repo(&state.workspace);
    
    // ═══════════════════════════════════════════════════════════════
    // HEADER
    // ═══════════════════════════════════════════════════════════════
    println!();
    log_task("════════════════════════════════════════════════════════");
    log_task(&format!("              {} - MODO AUTÓNOMO", BRAND));
    log_task("════════════════════════════════════════════════════════");
    println!();
    
    log_info(&format!("🤖 Modelo: {}", state.model.display_name.cyan()));
    log_info(&format!("📊 Contexto: {}k tokens", state.model.token_limit / 1000));
    
    if state.model.provider != "ollama" {
        if let Some(key) = state.current_key() {
            let masked = mask_key(&key);
            log_key(&format!("🔑 Key #{}/{}: {}", state.current_key_index + 1, state.keys_count(), masked));
        }
    }
    
    if dynamic_mode { 
        log_info(&format!("📂 Modo dinámico: máx {} archivos", max_files)); 
    }
    if state.mix_models { log_info("🔀 Mix models: ON"); }
    if state.auto_run { log_info("🔨 Auto-execute: ON (-e)"); }
    
    log_task("════════════════════════════════════════════════════════");
    
    // ═══════════════════════════════════════════════════════════════
    // CARGAR TODO.MD SI EXISTE
    // ═══════════════════════════════════════════════════════════════
    if state.todo_file.exists() && state.last_next_steps.is_empty() {
        let (steps, handoff) = state.load_todo();
        if let Some(s) = steps {
            log_todo("📋 Cargando estado previo de todo.md...");
            state.last_next_steps = s;
        }
        if let Some(h) = handoff {
            state.last_agent_handoff = h;
        }
    }

    // Preview de tarea   
    let task_preview = truncate_safe(&base_task, 300);
    println!("\n📝 Tarea: {}\n", task_preview.white());
    
    env::set_current_dir(&state.workspace).ok();

    // ═══════════════════════════════════════════════════════════════
    // MODO -e: EJECUTAR/COMPILAR PRIMERO
    // ═══════════════════════════════════════════════════════════════
    if state.auto_run {
        println!();
        log_info("🔨 ═══ MODO -e: VERIFICANDO COMPILACIÓN PRIMERO ═══");
        println!();
        
        match try_compile(&state.workspace, true) {
            CompileResult::Success => {
                log_ok("✅ Proyecto compila correctamente - continuando con tarea");
                state.last_compile_errors.clear();
            }
            CompileResult::Failed { errors } => {
                log_warn("⚠️ ¡ERRORES DE COMPILACIÓN DETECTADOS!");
                println!();
                
                // Mostrar primeras líneas de errores
                let error_lines: Vec<&str> = errors.lines().take(30).collect();
                for line in &error_lines {
                    if line.contains("error[") || line.contains("error:") {
                        println!("  {}", line.red());
                    } else {
                        println!("  {}", line.dimmed());
                    }
                }
                if errors.lines().count() > 30 {
                    println!("  ... {} líneas más", errors.lines().count() - 30);
                }
                println!();
                
                state.last_compile_errors = errors.clone();
                
                // ═══ LOOP DE CORRECCIÓN DE ERRORES ═══
                let mut fix_attempts = 0;
                const MAX_FIX_ATTEMPTS: usize = 5;
                
                while !state.last_compile_errors.is_empty() && fix_attempts < MAX_FIX_ATTEMPTS {
                    if should_exit() { 
                        state.save_todo_now();
                        return; 
                    }
                    
                    fix_attempts += 1;
                    state.iteration += 1;
                    
                    log_iter(&format!("═══ CORRIGIENDO ERRORES (intento {}/{}) ═══", 
                        fix_attempts, MAX_FIX_ATTEMPTS));
                    
                    // Usar EXECUTE_FIRST_PROMPT
                    let fix_prompt = format!(
                        "{}\n\n{}",
                        SYSTEM_PROMPT,
                        EXECUTE_FIRST_PROMPT
                            .replace("{errors}", &state.last_compile_errors)
                            .replace("{task}", &base_task)
                    );
                    
                    match run_aider(state, &fix_prompt, false, max_files, timeout_minutes) {
                        AiderResult::Success { files_modified, output, .. } => {
                            if !files_modified.is_empty() {
                                log_ok(&format!("📝 Modificados: {}", files_modified.join(", ")));
                                commit_after_changes(&state.workspace, &files_modified, state.iteration);
                            } else {
                                log_warn("⚠️ No se detectaron cambios en archivos");
                            }
                            
                            // Guardar progreso
                            if let Some(steps) = extract_next_steps(&output) {
                                state.last_next_steps = steps;
                            }
                            if let Some(handoff) = extract_agent_handoff(&output) {
                                state.last_agent_handoff = handoff;
                            }
                            state.save_todo_now();
                            
                            // Re-verificar compilación
                            println!();
                            log_info("🔨 Verificando si errores fueron corregidos...");
                            
                            match try_compile(&state.workspace, state.verbose) {
                                CompileResult::Success => {
                                    log_ok("✅ ¡ERRORES CORREGIDOS! Proyecto compila.");
                                    state.last_compile_errors.clear();
                                }
                                CompileResult::Failed { errors } => {
                                    let error_count = errors.lines()
                                        .filter(|l| l.contains("error[") || l.contains("error:"))
                                        .count();
                                    log_warn(&format!("Aún hay {} errores, reintentando...", error_count));
                                    state.last_compile_errors = errors;
                                }
                                CompileResult::NotProject => {
                                    state.last_compile_errors.clear();
                                }
                            }
                        }
                        AiderResult::ApiError(err) => {
                            log_error(&format!("⚡ Error de API: {}", err.description()));
                            if !handle_api_error_with_wait(state, &err) {
                                return;
                            }
                        }
                        AiderResult::Error(e) => {
                            log_warn(&format!("Error: {}", e));
                        }
                        AiderResult::Killed => {
                            state.save_todo_now();
                            return;
                        }
                    }
                    
                    // Pausa entre intentos (no para Ollama)
                    if !state.last_compile_errors.is_empty() && state.model.provider != "ollama" {
                        wait_with_countdown(3, "antes de reintentar");
                    }
                }
                
                if !state.last_compile_errors.is_empty() {
                    log_warn("⚠️ No se corrigieron todos los errores, pero continuamos con la tarea...");
                }
            }
            CompileResult::NotProject => {
                log_info("No es proyecto Rust/Cargo, saltando verificación");
            }
        }
        
        println!();
        log_info("═══ FIN DE VERIFICACIÓN INICIAL ═══");
        println!();
    }

    // ═══════════════════════════════════════════════════════════════
    // LOOP PRINCIPAL DE ITERACIONES
    // ═══════════════════════════════════════════════════════════════
    loop {
        if should_exit() { 
            log_warn("Salida solicitada");
            state.flush_pending_edits();
            state.save_todo_now();
            return; 
        }
        
        state.iteration += 1;
        println!();
        log_iter(&format!("═══════════════ ITERACIÓN {} ═══════════════", state.iteration));
        
        // Mostrar key actual para APIs cloud
        if state.model.provider != "ollama" {
            if let Some(key) = state.current_key() {
                let masked = mask_key(&key);
                log_key(&format!("🔑 Key #{}/{}: {}", state.current_key_index + 1, state.keys_count(), masked));
            }
        }

        // Backup pre-iteración
        commit_before_iteration(&state.workspace, state.iteration);

        // ═══ CONSTRUIR PROMPT USANDO build_iteration_prompt ═══
        let task = build_iteration_prompt(state, &base_task);
        
        log_debug(&format!("Prompt: {} chars", task.len()), state.verbose);

        match run_aider(state, &task, dynamic_mode, max_files, timeout_minutes) {
            AiderResult::Success { output, files_modified, tokens_sent, tokens_received } => {
                state.session_started = true;
                state.reset_errors();
                state.restore_original_model();
                state.total_tokens_sent += tokens_sent;
                state.total_tokens_received += tokens_received;

                // GUARDAR INMEDIATAMENTE
                if !files_modified.is_empty() {
                    log_ok(&format!("💾 Guardados: {}", files_modified.join(", ")));
                    commit_after_changes(&state.workspace, &files_modified, state.iteration);
                } else {
                    log_warn("⚠️ No se detectaron cambios - verificar output del modelo");
                }

                // Extraer y guardar progreso
                if let Some(steps) = extract_next_steps(&output) {
                    state.last_next_steps = steps;
                }
                if let Some(handoff) = extract_agent_handoff(&output) {
                    state.last_agent_handoff = handoff;
                }
                state.save_todo_now();

                // Verificar si piensa que terminó
                let thinks_complete = output.to_lowercase().contains("task completed");
                
                if thinks_complete {
                    let next_steps = extract_next_steps(&output);
                    if !is_truly_complete(&output, &next_steps) {
                        log_warn("⚠️ Dice COMPLETED pero hay tareas pendientes");
                        continue;
                    }
                    
                    // Verificación final si -e está activo
                    if state.auto_run {
                        match try_compile(&state.workspace, state.verbose) {
                            CompileResult::Success => {
                                state.last_compile_errors.clear();
                                log_ok("✅ Compilación final exitosa");
                                let _ = try_run(&state.workspace, state.verbose);
                            }
                            CompileResult::Failed { errors } => { 
                                log_warn("Compilación final falló - corrigiendo"); 
                                state.last_compile_errors = errors; 
                                continue; 
                            }
                            CompileResult::NotProject => {}
                        }
                    }
                    
                    finalize_task(state);
                    return;
                }
                
                // Auto-compile después de cambios si -e activo
                if state.auto_run && !files_modified.is_empty() {
                    match try_compile(&state.workspace, state.verbose) {
                        CompileResult::Success => {
                            log_ok("✅ Compila OK");
                            state.last_compile_errors.clear();
                        }
                        CompileResult::Failed { errors } => {
                            let error_count = errors.lines()
                                .filter(|l| l.contains("error["))
                                .count();
                            log_warn(&format!("⚠️ {} errores de compilación", error_count));
                            state.last_compile_errors = errors;
                        }
                        CompileResult::NotProject => {}
                    }
                }
            }

            AiderResult::ApiError(err) => {
                state.record_error();
                log_error(&format!("⚡ API Error: {}", err.description()));
                
                state.flush_pending_edits();
                state.save_todo_now();
                
                if !handle_api_error_with_wait(state, &err) {
                    return;
                }
            }

            AiderResult::Error(e) => {
                log_warn(&format!("Error: {}", e));
                state.record_error();
                state.flush_pending_edits();
                state.save_todo_now();
                
                if state.consecutive_errors >= 5 { 
                    log_error("Demasiados errores consecutivos"); 
                    return; 
                }
                
                // Solo esperar si NO es Ollama
                if state.model.provider != "ollama" {
                    wait_with_countdown(10, "después de error");
                }
            }

            AiderResult::Killed => {
                state.flush_pending_edits();
                state.save_todo_now();
                return;
            }
        }
        
        // Pausa entre iteraciones (no para Ollama)
        if state.model.provider != "ollama" {
            thread::sleep(Duration::from_secs(WAIT_API_BETWEEN_REQUESTS));
        }
    }
}

/// Construye el prompt para cada iteración usando las constantes correctas
fn build_iteration_prompt(state: &AppState, base_task: &str) -> String {
    let mut prompt = String::new();
    
    // ═══════════════════════════════════════════════════════════════
    // SYSTEM PROMPT - Siempre al inicio
    // ═══════════════════════════════════════════════════════════════
    prompt.push_str(SYSTEM_PROMPT);
    prompt.push_str(EXAMPLES_SECTION);
    prompt.push_str("\n\n");
    
    // ═══════════════════════════════════════════════════════════════
    // ERRORES DE COMPILACIÓN - Máxima prioridad
    // ═══════════════════════════════════════════════════════════════
    if !state.last_compile_errors.is_empty() {
        let error_prompt = FIX_COMPILE_PROMPT.replace("{errors}", &state.last_compile_errors);
        prompt.push_str(&error_prompt);
        prompt.push_str("\n\n");
        prompt.push_str("After fixing errors, continue with:\n\n");
    }
    
    // ═══════════════════════════════════════════════════════════════
    // CONTEXTO PREVIO (AGENT_HANDOFF)
    // ═══════════════════════════════════════════════════════════════
    if !state.last_agent_handoff.is_empty() {
        prompt.push_str("═══ PREVIOUS CONTEXT ═══\n");
        prompt.push_str(&state.last_agent_handoff);
        prompt.push_str("\n\n");
    }
    
    // ═══════════════════════════════════════════════════════════════
    // TAREAS PENDIENTES O TAREA PRINCIPAL
    // ═══════════════════════════════════════════════════════════════
    if !state.last_next_steps.is_empty() {
        prompt.push_str("═══ PENDING TASKS ═══\n");
        prompt.push_str(&state.last_next_steps);
        prompt.push_str("\n\n");
        prompt.push_str(CONTINUE_PROMPT);
    } else {
        prompt.push_str("═══ MAIN TASK ═══\n");
        prompt.push_str(base_task);
    }
    
    // ═══════════════════════════════════════════════════════════════
    // SUFIJO - Siempre al final
    // ═══════════════════════════════════════════════════════════════
    prompt.push_str(TASK_SUFFIX);
    
    prompt
}

fn finalize_task(state: &mut AppState) {
    // Mover todo.md a .done
    if state.todo_file.exists() { 
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let done_file = state.todo_file.with_extension(format!("md.done.{}", timestamp));
        let _ = fs::rename(&state.todo_file, &done_file); 
    }
    
    git_commit_changes(&state.workspace, &format!("{}: task completed ✓", APP_NAME));
    
    println!();
    println!("{}", "═".repeat(60).green());
    println!("{}", "            🎉 TAREA COMPLETADA 🎉".green().bold());
    println!("{}", "═".repeat(60).green());
    println!();
    log_info(&format!("📝 Archivos modificados: {}", state.total_files_modified));
    log_info(&format!("📊 Tokens usados: {}k enviados, {}k recibidos", 
        state.total_tokens_sent / 1000, 
        state.total_tokens_received / 1000
    ));
    log_info(&format!("🔄 Iteraciones: {}", state.iteration));
    println!();
}

fn show_status(state: &AppState, models: &[DynamicModel]) {
    print_banner();
    
    println!("{}", "═══ ESTADO DEL SISTEMA ═══".white().bold());
    println!();
    
    // APIs disponibles
    println!("{}", "🔑 API Keys Configuradas:".yellow().bold());
    
    let providers = [
        ("Gemini", &state.gemini_keys),
        ("Groq", &state.groq_keys),
        ("DeepSeek", &state.deepseek_keys),
        ("OpenRouter", &state.openrouter_keys),
        ("Chutes", &state.chutes_keys),
        ("SambaNova", &state.sambanova_keys),
        ("Cerebras", &state.cerebras_keys),
        ("Together", &state.together_keys),
        ("Fireworks", &state.fireworks_keys),
        ("Cohere", &state.cohere_keys),
        ("HuggingFace", &state.huggingface_keys),
        ("Mistral", &state.mistral_keys),
        ("Novita", &state.novita_keys),
        ("HyperBolic", &state.hyperbolic_keys),
    ];
    
    for (name, keys) in providers {
        if !keys.is_empty() {
            println!("  {} {} keys", format!("✓ {}:", name).green(), keys.len());
        }
    }
    
    println!("  {} {}", 
        if is_ollama_running() { "✓ Ollama:".green() } else { "✗ Ollama:".red() },
        if is_ollama_running() { "corriendo" } else { "no corriendo" }
    );
    
    if !state.banned_keys.is_empty() { 
        println!("  {}: {}", "⚠ Baneadas".red(), state.banned_keys.len()); 
    }
    
    // Modelos por proveedor
    println!();
    println!("{}", "🤖 Modelos Disponibles:".yellow().bold());
    let mut by_provider: HashMap<&str, usize> = HashMap::new();
    for m in models {
        *by_provider.entry(m.provider.as_str()).or_insert(0) += 1;
    }
    for (provider, count) in by_provider.iter() {
        println!("  {}: {} modelos", provider.to_uppercase(), count);
    }
    println!("  {}: {} modelos", "TOTAL".bold(), models.len());
    
    // Proyecto actual
    println!();
    println!("{}", "📁 Proyecto Actual:".yellow().bold());
    let files = get_source_files(&state.workspace);
    let total_tokens: usize = files.iter().map(|f| count_tokens(f)).sum();
    println!("  Directorio: {}", state.workspace.display());
    println!("  Archivos fuente: {}", files.len());
    println!("  Tokens estimados: ~{}k", total_tokens / 1000);
    
    // Estado de todo.md
    if state.todo_file.exists() { 
        println!();
        println!("{}", "📋 Tareas Pendientes (todo.md):".yellow().bold());
        if let Ok(content) = fs::read_to_string(&state.todo_file) {
            for line in content.lines().take(10) {
                println!("  {}", line.dimmed());
            }
            if content.lines().count() > 10 {
                println!("  ... y {} líneas más", content.lines().count() - 10);
            }
        }
    }
    
    println!();
}

/// Maneja errores de API con recuperación

fn handle_api_error_with_recovery(
    state: &mut AppState, 
    err: &ApiError, 
    current_retries: usize,
    retries: &mut usize
) -> bool {
    const MAX_RETRIES: usize = 5;
    
    // Usar current_retries para logging y decisiones
    log_debug(&format!("Reintento {}/{} para error {:?}", current_retries, MAX_RETRIES, err), state.verbose);
    
    // Manejo especial para Ollama
    if state.model.provider == "ollama" || state.model.provider == "llama-cpp" {
        *retries += 1;
        
        // Después de varios reintentos, ser más agresivo
        if current_retries >= 2 {
            log_warn(&format!("Reintento {} - aplicando medidas más agresivas", current_retries));
        }
        
        if matches!(err, ApiError::TokenLimit) {
            log_warn("Reduciendo contexto para modelo local...");
            state.model.token_limit = state.model.token_limit * 3 / 4;
            
            if *retries >= MAX_RETRIES || current_retries >= MAX_RETRIES {
                log_error("Demasiados reintentos por token limit");
                return false;
            }
            return true;
        }
        
        if *retries >= MAX_RETRIES {
            log_error(&format!("Modelo local falló después de {} intentos", MAX_RETRIES));
            return false;
        }
        
        if matches!(err, ApiError::Connection | ApiError::Timeout) {
            log_info("Reiniciando servicio local...");
            if state.model.provider == "ollama" {
                let _ = Command::new("pkill").args(["-f", "ollama"]).output();
                thread::sleep(Duration::from_secs(2));
                start_ollama();
            }
            // Para llama-cpp, el usuario debe reiniciar manualmente
        }
        
        return true;
    }
    
    // Para APIs en la nube
    if current_retries >= MAX_RETRIES {
        log_error(&format!("Demasiados reintentos ({}) para este error", current_retries));
        return false;
    }
    
    if err.is_permanent_ban() {
        state.mark_suspended(true);
        if !state.rotate_key() { 
            return false;
        }
        thread::sleep(Duration::from_secs(2));
        return true;
    }
    
    if err.should_rotate_key() && state.rotate_key() {
        log_info("✓ Key rotada");
        thread::sleep(Duration::from_secs(2));
        return true;
    }
    
    let wait = err.wait_seconds();
    if wait > 0 { 
        wait_with_countdown(wait, &format!("después de error (intento {})", current_retries));
    }
    
    !should_exit()
}

// ═══════════════════════════════════════════════════════════════════════════════
// CHUTES.AI API
// ═══════════════════════════════════════════════════════════════════════════════

fn fetch_chutes_models(api_key: Option<&str>) -> Vec<DynamicModel> {
    let key = get_api_key_from_sources("CHUTES_API_KEY", api_key);
    if key.is_empty() { return Vec::new(); }
    
    let output = Command::new("curl")
        .args([
            "-s", "--connect-timeout", "10",
            "-H", &format!("Authorization: Bearer {}", key),
            "https://chutes.ai/api/v1/models"
        ])
        .output();
    
    match output {
        Ok(o) if o.status.success() => parse_chutes_models(&String::from_utf8_lossy(&o.stdout)),
        _ => {
            // Fallback: modelos conocidos de Chutes.ai
            vec![
                create_api_model("chutes", "deepseek-ai/DeepSeek-R1", "DeepSeek R1", 128_000, true),
                create_api_model("chutes", "deepseek-ai/DeepSeek-V3", "DeepSeek V3", 128_000, true),
                create_api_model("chutes", "Qwen/Qwen3-235B-A22B", "Qwen 3 235B", 128_000, false),
                create_api_model("chutes", "mistralai/Codestral-2501", "Codestral 2501", 256_000, false),
            ]
        }
    }
}

fn parse_chutes_models(json: &str) -> Vec<DynamicModel> {
    let mut models = Vec::new();
    
    // Parser simple para formato {"data": [{"id": "...", ...}]}
    for line in json.split(&['{', '}'][..]) {
        let line = line.trim();
        if line.contains("\"id\"") {
            if let Some(id) = extract_json_string_value(line, "id") {
                if !id.contains("embedding") && !id.contains("vision") {
                    let display = id.split('/').last().unwrap_or(&id).to_string();
                    let ctx = if id.contains("deepseek") { 128_000 } 
                             else if id.contains("codestral") { 256_000 }
                             else { 128_000 };
                    let is_free = id.to_lowercase().contains("deepseek-r1");
                    
                    models.push(DynamicModel {
                        name: display.clone(),
                        display_name: format!("{} (Chutes)", display),
                        provider: "chutes".to_string(),
                        token_limit: ctx,
                        is_free,

                    });
                }
            }
        }
    }
    
    models
}

// ═══════════════════════════════════════════════════════════════════════════════
// SAMBANOVA API (Free Tier - Llama 405B!)
// ═══════════════════════════════════════════════════════════════════════════════

fn fetch_sambanova_models(api_key: Option<&str>) -> Vec<DynamicModel> {
    let key = get_api_key_from_sources("SAMBANOVA_API_KEY", api_key);
    if key.is_empty() { return Vec::new(); }
    
    let output = Command::new("curl")
        .args([
            "-s", "--connect-timeout", "10",
            "-H", &format!("Authorization: Bearer {}", key),
            "https://api.sambanova.ai/v1/models"
        ])
        .output();
    
    match output {
        Ok(o) if o.status.success() => parse_openai_compatible_models(&String::from_utf8_lossy(&o.stdout), "sambanova"),
        _ => {
            // Fallback conocidos
            vec![
                create_api_model("sambanova", "Meta-Llama-3.1-405B-Instruct", "Llama 3.1 405B", 128_000, true),
                create_api_model("sambanova", "Meta-Llama-3.1-70B-Instruct", "Llama 3.1 70B", 128_000, true),
                create_api_model("sambanova", "Meta-Llama-3.1-8B-Instruct", "Llama 3.1 8B", 128_000, true),
            ]
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// CEREBRAS API (Ultra-fast inference)
// ═══════════════════════════════════════════════════════════════════════════════

fn fetch_cerebras_models(api_key: Option<&str>) -> Vec<DynamicModel> {
    let key = get_api_key_from_sources("CEREBRAS_API_KEY", api_key);
    if key.is_empty() { return Vec::new(); }
    
    let output = Command::new("curl")
        .args([
            "-s", "--connect-timeout", "10",
            "-H", &format!("Authorization: Bearer {}", key),
            "https://api.cerebras.ai/v1/models"
        ])
        .output();
    
    match output {
        Ok(o) if o.status.success() => parse_openai_compatible_models(&String::from_utf8_lossy(&o.stdout), "cerebras"),
        _ => {
            vec![
                create_api_model("cerebras", "llama3.1-70b", "Llama 3.1 70B (ultrafast)", 128_000, true),
                create_api_model("cerebras", "llama3.1-8b", "Llama 3.1 8B (ultrafast)", 128_000, true),
            ]
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TOGETHER AI API
// ═══════════════════════════════════════════════════════════════════════════════

fn fetch_together_models(api_key: Option<&str>) -> Vec<DynamicModel> {
    let key = get_api_key_from_sources("TOGETHER_API_KEY", api_key);
    if key.is_empty() { return Vec::new(); }
    
    let output = Command::new("curl")
        .args([
            "-s", "--connect-timeout", "10",
            "-H", &format!("Authorization: Bearer {}", key),
            "https://api.together.xyz/v1/models"
        ])
        .output();
    
    match output {
        Ok(o) if o.status.success() => parse_together_models(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new()
    }
}

fn parse_together_models(json: &str) -> Vec<DynamicModel> {
    let mut models = Vec::new();
    let mut current_id = String::new();
    let mut current_ctx: usize = 128_000;
    
    for line in json.split(&['{', '}', ','][..]) {
        let line = line.trim();
        
        if line.contains("\"id\"") {
            if let Some(id) = extract_json_string_value(line, "id") {
                // Filtrar solo modelos de texto/código
                let dominated_dominated_lower = id.to_lowercase();
                if !dominated_dominated_lower.contains("embed") && !dominated_dominated_lower.contains("vision") 
                   && !dominated_dominated_lower.contains("image") && !dominated_dominated_lower.contains("clip") {
                    current_id = id;
                }
            }
        }
        
        if line.contains("\"token_limit\"") || line.contains("\"max_tokens\"") {
            if let Some(ctx) = extract_json_number_value(line, "token_limit")
                .or_else(|| extract_json_number_value(line, "max_tokens")) {
                current_ctx = ctx;
            }
        }
        
        if !current_id.is_empty() && (line.contains("\"type\"") || line.contains("\"created\"")) {
            let display = current_id.split('/').last().unwrap_or(&current_id)
                .replace("-", " ").replace("_", " ");
            
            models.push(DynamicModel {
                name: current_id.split('/').last().unwrap_or(&current_id).to_string(),
                display_name: format!("{} (Together)", display),
                provider: "together".to_string(),
                token_limit: current_ctx,
                is_free: false,

            });
            
            current_id.clear();
            current_ctx = 128_000;
        }
    }
    
    models
}

// ═══════════════════════════════════════════════════════════════════════════════
// FIREWORKS AI API
// ═══════════════════════════════════════════════════════════════════════════════

fn fetch_fireworks_models(api_key: Option<&str>) -> Vec<DynamicModel> {
    let key = get_api_key_from_sources("FIREWORKS_API_KEY", api_key);
    if key.is_empty() { return Vec::new(); }
    
    let output = Command::new("curl")
        .args([
            "-s", "--connect-timeout", "10",
            "-H", &format!("Authorization: Bearer {}", key),
            "https://api.fireworks.ai/inference/v1/models"
        ])
        .output();
    
    match output {
        Ok(o) if o.status.success() => parse_openai_compatible_models(&String::from_utf8_lossy(&o.stdout), "fireworks"),
        _ => Vec::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// COHERE API
// ═══════════════════════════════════════════════════════════════════════════════

fn fetch_cohere_models(api_key: Option<&str>) -> Vec<DynamicModel> {
    let key = get_api_key_from_sources("COHERE_API_KEY", api_key);
    if key.is_empty() { return Vec::new(); }
    
    let output = Command::new("curl")
        .args([
            "-s", "--connect-timeout", "10",
            "-H", &format!("Authorization: Bearer {}", key),
            "https://api.cohere.ai/v1/models"
        ])
        .output();
    
    match output {
        Ok(o) if o.status.success() => parse_cohere_models(&String::from_utf8_lossy(&o.stdout)),
        _ => {
            vec![
                create_api_model("cohere", "command-r-plus", "Command R+", 128_000, false),
                create_api_model("cohere", "command-r", "Command R", 128_000, true),
                create_api_model("cohere", "command", "Command", 4_096, true),
            ]
        }
    }
}

fn parse_cohere_models(json: &str) -> Vec<DynamicModel> {
    let mut models = Vec::new();
    
    for line in json.split(&['{', '}'][..]) {
        if let Some(name) = extract_json_string_value(line, "name") {
            if name.starts_with("command") {
                let ctx = if name.contains("r-plus") || name.contains("r") { 128_000 } else { 4_096 };
                let is_free = !name.contains("plus");
                
                models.push(DynamicModel {
                    name: name.clone(),
                    display_name: format!("{} (Cohere)", name.replace("-", " ")),
                    provider: "cohere".to_string(),
                    token_limit: ctx,
                    is_free,
                });
            }
        }
    }
    
    models
}

// ═══════════════════════════════════════════════════════════════════════════════
// HUGGING FACE INFERENCE API
// ═══════════════════════════════════════════════════════════════════════════════

/// HUGGING FACE INFERENCE API
fn fetch_huggingface_models(api_key: Option<&str>) -> Vec<DynamicModel> {
    let key = get_api_key_from_sources("HUGGINGFACE_API_KEY", api_key);
    let hf_token = get_api_key_from_sources("HF_TOKEN", None);
    
    // Usar cualquiera de las dos keys
    let effective_key = if !key.is_empty() { key } else { hf_token };
    
    if effective_key.is_empty() { 
        return Vec::new(); 
    }
    
    // HuggingFace no tiene endpoint fácil, usar modelos conocidos
    vec![
        create_api_model("huggingface", "meta-llama/Llama-3.2-3B-Instruct", "Llama 3.2 3B", 128_000, true),
        create_api_model("huggingface", "meta-llama/Llama-3.1-8B-Instruct", "Llama 3.1 8B", 128_000, true),
        create_api_model("huggingface", "mistralai/Mistral-7B-Instruct-v0.3", "Mistral 7B", 32_000, true),
        create_api_model("huggingface", "Qwen/Qwen2.5-Coder-32B-Instruct", "Qwen 2.5 Coder 32B", 128_000, true),
        create_api_model("huggingface", "bigcode/starcoder2-15b", "StarCoder2 15B", 16_000, true),
    ]
}

// ═══════════════════════════════════════════════════════════════════════════════
// MISTRAL API
// ═══════════════════════════════════════════════════════════════════════════════

fn fetch_mistral_models(api_key: Option<&str>) -> Vec<DynamicModel> {
    let key = get_api_key_from_sources("MISTRAL_API_KEY", api_key);
    if key.is_empty() { return Vec::new(); }
    
    let output = Command::new("curl")
        .args([
            "-s", "--connect-timeout", "10",
            "-H", &format!("Authorization: Bearer {}", key),
            "https://api.mistral.ai/v1/models"
        ])
        .output();
    
    match output {
        Ok(o) if o.status.success() => parse_mistral_models(&String::from_utf8_lossy(&o.stdout)),
        _ => {
            vec![
                create_api_model("mistral", "codestral-latest", "Codestral", 256_000, false),
                create_api_model("mistral", "mistral-large-latest", "Mistral Large", 128_000, false),
                create_api_model("mistral", "mistral-small-latest", "Mistral Small", 32_000, true),
                create_api_model("mistral", "open-mistral-nemo", "Mistral Nemo", 128_000, true),
            ]
        }
    }
}

fn parse_mistral_models(json: &str) -> Vec<DynamicModel> {
    let mut models = Vec::new();
    
    for line in json.split(&['{', '}'][..]) {
        if let Some(id) = extract_json_string_value(line, "id") {
            if !id.contains("embed") {
                let ctx = if id.contains("codestral") { 256_000 } 
                         else if id.contains("small") { 32_000 }
                         else { 128_000 };
                let is_free = id.contains("nemo") || id.contains("small");
                
                let display = id.replace("-latest", "").replace("-", " ");
                
                models.push(DynamicModel {
                    name: id.clone(),
                    display_name: format!("{} (Mistral)", display),
                    provider: "mistral".to_string(),
                    token_limit: ctx,
                    is_free,
                });
            }
        }
    }
    
    models
}

// ═══════════════════════════════════════════════════════════════════════════════
// NOVITA AI API
// ═══════════════════════════════════════════════════════════════════════════════

fn fetch_novita_models(api_key: Option<&str>) -> Vec<DynamicModel> {
    let key = get_api_key_from_sources("NOVITA_API_KEY", api_key);
    if key.is_empty() { return Vec::new(); }
    
    let output = Command::new("curl")
        .args([
            "-s", "--connect-timeout", "10",
            "-H", &format!("Authorization: Bearer {}", key),
            "https://api.novita.ai/v3/openai/models"
        ])
        .output();
    
    match output {
        Ok(o) if o.status.success() => parse_openai_compatible_models(&String::from_utf8_lossy(&o.stdout), "novita"),
        _ => {
            vec![
                create_api_model("novita", "deepseek-ai/deepseek-v3", "DeepSeek V3", 128_000, false),
                create_api_model("novita", "qwen/qwen-2.5-coder-32b-instruct", "Qwen 2.5 Coder 32B", 128_000, false),
            ]
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// HYPERBOLIC API
// ═══════════════════════════════════════════════════════════════════════════════

fn fetch_hyperbolic_models(api_key: Option<&str>) -> Vec<DynamicModel> {
    let key = get_api_key_from_sources("HYPERBOLIC_API_KEY", api_key);
    if key.is_empty() { return Vec::new(); }
    
    let output = Command::new("curl")
        .args([
            "-s", "--connect-timeout", "10",
            "-H", &format!("Authorization: Bearer {}", key),
            "https://api.hyperbolic.xyz/v1/models"
        ])
        .output();
    
    match output {
        Ok(o) if o.status.success() => parse_openai_compatible_models(&String::from_utf8_lossy(&o.stdout), "hyperbolic"),
        _ => {
            vec![
                create_api_model("hyperbolic", "deepseek-ai/DeepSeek-R1", "DeepSeek R1", 128_000, false),
                create_api_model("hyperbolic", "meta-llama/Llama-3.3-70B-Instruct", "Llama 3.3 70B", 128_000, false),
            ]
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// API HELPERS
// ═══════════════════════════════════════════════════════════════════════════════

/// Busca API key en múltiples fuentes
fn get_api_key_from_sources(env_name: &str, provided: Option<&str>) -> String {
    // 1. Proporcionada directamente
    if let Some(k) = provided {
        if k.len() > 10 { return k.to_string(); }
    }
    
    // 2. Variable de entorno
    if let Ok(k) = env::var(env_name) {
        if k.len() > 10 { return k; }
    }
    
    // 3. Archivo de configuración
    let keys_file = dirs::home_dir()
        .unwrap_or_default()
        .join(".config/rustmind/keys.env");  // Nuevo path
    
    // Fallback al path viejo
    let keys_file = if keys_file.exists() { keys_file } else {
        dirs::home_dir().unwrap_or_default().join(".config/rustmind/keys.env")
    };
    
    if let Ok(content) = fs::read_to_string(&keys_file) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with(env_name) && line.contains('=') {
                if let Some(value) = line.split('=').nth(1) {
                    let clean = value.trim().trim_matches('"').trim_matches('\'');
                    if clean.len() > 10 { return clean.to_string(); }
                }
            }
        }
    }
    
    String::new()
}

/// Crea un DynamicModel para una API
fn create_api_model(provider: &str, id: &str, display: &str, ctx: usize, is_free: bool) -> DynamicModel {
    let short_name = id.split('/').last().unwrap_or(id).to_string();
    
    DynamicModel {
        name: short_name.clone(),
        display_name: format!("{} ({})", display, provider),
        provider: provider.to_string(),
        token_limit: ctx,
        is_free,
    }
}

/// Extrae valor string de JSON simple
fn extract_json_string_value(text: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    if let Some(pos) = text.find(&pattern) {
        let after = &text[pos + pattern.len()..];
        if let Some(colon) = after.find(':') {
            let value_part = after[colon + 1..].trim();
            if value_part.starts_with('"') {
                if let Some(end) = value_part[1..].find('"') {
                    return Some(value_part[1..end + 1].to_string());
                }
            }
        }
    }
    None
}

/// Extrae valor numérico de JSON simple
fn extract_json_number_value(text: &str, key: &str) -> Option<usize> {
    let pattern = format!("\"{}\"", key);
    if let Some(pos) = text.find(&pattern) {
        let after = &text[pos + pattern.len()..];
        if let Some(colon) = after.find(':') {
            let value_part = after[colon + 1..].trim();
            let num_str: String = value_part.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            return num_str.parse().ok();
        }
    }
    None
}

/// Parser genérico para APIs compatibles con OpenAI
fn parse_openai_compatible_models(json: &str, provider: &str) -> Vec<DynamicModel> {
    let mut models = Vec::new();
    let mut current_id = String::new();
    let mut current_ctx: usize = 128_000;
    
    for line in json.split(&['{', '}', ','][..]) {
        let line = line.trim();
        
        if line.contains("\"id\"") {
            if let Some(id) = extract_json_string_value(line, "id") {
                let lower = id.to_lowercase();
                // Filtrar modelos no útiles
                if !lower.contains("embed") && !lower.contains("vision") 
                   && !lower.contains("whisper") && !lower.contains("tts")
                   && !lower.contains("image") && !lower.contains("dall") {
                    current_id = id;
                }
            }
        }
        
        if let Some(ctx) = extract_json_number_value(line, "token_limit")
            .or_else(|| extract_json_number_value(line, "max_tokens"))
            .or_else(|| extract_json_number_value(line, "context_window")) {
            current_ctx = ctx;
        }
        
        if !current_id.is_empty() && 
           (line.contains("\"created\"") || line.contains("\"owned_by\"") || line.contains("\"object\"")) {
            let display = current_id.split('/').last().unwrap_or(&current_id)
                .replace("-", " ").replace("_", " ");
            
            models.push(DynamicModel {
                name: current_id.split('/').last().unwrap_or(&current_id).to_string(),
                display_name: format!("{} ({})", display, provider),
                provider: provider.to_string(),
                token_limit: current_ctx,
                is_free: false,
            });
            
            current_id.clear();
            current_ctx = 128_000;
        }
    }
    
    models.sort_by(|a, b| a.name.cmp(&b.name));
    models.dedup_by(|a, b| a.name == b.name);
    models
}

/// Aplica edits pendientes con backup de seguridad
fn apply_pending_edits_with_backup(workspace: &Path, edits: &[PendingEdit]) -> usize {
    let mut applied = 0;

    for edit in edits {
        let path = workspace.join(&edit.filename);
        
        if !path.exists() {
            // Si search está vacío, es contenido nuevo - crear archivo
            if edit.search.trim().is_empty() && !edit.replace.trim().is_empty() {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if fs::write(&path, &edit.replace).is_ok() {
                    log_ok(&format!("✓ Creado: {}", edit.filename));
                    applied += 1;
                }
            }
            continue;
        }

        if let Ok(content) = fs::read_to_string(&path) {
            let search = edit.search.trim();
            
            // Crear backup antes de modificar
            let backup_path = path.with_extension("orig.bak");
            let _ = fs::write(&backup_path, &content);
            
            if content.contains(search) {
                let new = content.replace(search, edit.replace.trim());
                
                // Validar antes de escribir
                if let Err(e) = validate_change_safety(&content, &new, &edit.filename) {
                    log_warn(&format!("Cambio rechazado en {}: {}", edit.filename, e));
                    let _ = fs::remove_file(&backup_path);
                    continue;
                }
                
                if new != content {
                    if let Err(e) = fs::write(&path, &new) {
                        log_error(&format!("Error escribiendo {}: {}", edit.filename, e));
                        // Restaurar backup
                        let _ = fs::copy(&backup_path, &path);
                    } else {
                        log_ok(&format!("💾 Recovered: {} ({} → {} líneas)", 
                            edit.filename,
                            content.lines().count(),
                            new.lines().count()
                        ));
                        applied += 1;
                    }
                }
            }
            
            // Limpiar backup
            let _ = fs::remove_file(&backup_path);
        }
    }

    applied
}

/// Maneja errores de API (wrapper para uso en lugares que no necesitan recovery completo)
fn handle_api_error(state: &mut AppState, err: &ApiError) {
    state.record_error();
    
    if err.is_permanent_ban() {
        state.mark_suspended(true);
        let _ = state.rotate_key();
    } else if err.should_rotate_key() {
        let _ = state.rotate_key();
    }
    
    let wait = err.wait_seconds();
    if wait > 0 && wait <= 10 {
        thread::sleep(Duration::from_secs(wait));
    }
}
/// Maneja errores de API con cooldown configurable
fn handle_api_error_with_cooldown(
    state: &mut AppState, 
    err: &ApiError,
    api_config: &mut ApiConfig,
) -> bool {
    // APIs locales: sin espera, reintentar inmediatamente
    if api_config.is_local() {
        match err {
            ApiError::TokenLimit => {
                log_warn("Token limit en API local - reduciendo contexto");
                state.model.token_limit = state.model.token_limit * 3 / 4;
            }
            ApiError::Connection | ApiError::Timeout => {
                log_warn(&format!("{} no responde - verificando...", api_config.name));
                
                // Para Ollama, intentar reiniciar
                if api_config.name == "ollama" && !is_ollama_running() {
                    log_info("Reiniciando Ollama...");
                    restart_ollama_with_config("");
                }
            }
            _ => {
                log_warn(&format!("Error en {}: {:?}", api_config.name, err));
            }
        }
        return true;  // Siempre reintentar para locales
    }
    
    // APIs cloud: usar cooldown con backoff
    api_config.increment_cooldown();
    let wait_time = api_config.get_cooldown();
    
    log_warn(&format!("⏳ Error en {} - cooldown {}s (backoff activo)", 
        api_config.name, wait_time));
    
    match err {
        ApiError::PermissionDenied | ApiError::KeyInvalid => {
            log_error("🚫 Key inválida o baneada");
            state.mark_suspended(true);
            
            // Intentar rotar key
            if state.rotate_key() {
                log_ok("✓ Key rotada");
                api_config.reset_cooldown();  // Reset cooldown después de rotar
                wait_with_countdown(5, "después de rotar key");
                return !should_exit();
            } else {
                log_error("❌ No hay más keys disponibles");
                return false;
            }
        }
        
        ApiError::QuotaExhausted => {
            log_warn("📊 Cuota agotada");
            
            // Intentar rotar key
            if state.rotate_key() {
                log_ok("✓ Key rotada");
                api_config.reset_cooldown();
                wait_with_countdown(wait_time, "después de rotar key");
                return !should_exit();
            }
            
            // Si no hay más keys, esperar el cooldown completo
            wait_with_countdown(wait_time, &format!("cooldown de {}", api_config.name));
        }
        
        ApiError::RateLimitTemporary => {
            log_warn("⏱️ Rate limit temporal");
            wait_with_countdown(wait_time, "rate limit");
        }
        
        ApiError::ModelOverloaded => {
            log_warn("🔥 Modelo sobrecargado");
            wait_with_countdown(wait_time.max(20), "modelo sobrecargado");
        }
        
        ApiError::ServiceUnavailable => {
            log_warn("🔧 Servicio no disponible");
            wait_with_countdown(wait_time, "servicio no disponible");
        }
        
        ApiError::Connection | ApiError::Timeout => {
            log_warn("🌐 Error de conexión");
            wait_with_countdown(15, "reconectando");
        }
        
        _ => {
            wait_with_countdown(wait_time, "");
        }
    }
    
    // Verificar límite de errores
    if state.consecutive_errors >= 5 {
        log_warn("⚠️ Muchos errores consecutivos - pausa extendida");
        api_config.increment_cooldown();  // Incrementar más
        wait_with_countdown(api_config.get_cooldown(), "pausa extendida");
        state.reset_errors();
    }
    
    !should_exit()
}


/// Parser para múltiples formatos de diff
fn process_all_diff_formats(workspace: &Path, output: &str, verbose: bool) -> Vec<String> {
    let mut modified_files = Vec::new();
    
    // 1. Intentar SEARCH/REPLACE (preferido)
    let sr_edits = extract_search_replace_edits(output);
    for edit in &sr_edits {
        if apply_single_edit_safely(workspace, edit, verbose) {
            if !modified_files.contains(&edit.filename) {
                modified_files.push(edit.filename.clone());
            }
        }
    }
    
    // 2. Intentar Unified Diff (@@...@@)
    if modified_files.is_empty() {
        let unified_edits = extract_unified_diff_edits_improved(output);
        for edit in &unified_edits {
            if apply_unified_diff_edit(workspace, edit, verbose) {
                if !modified_files.contains(&edit.filename) {
                    modified_files.push(edit.filename.clone());
                }
            }
        }
    }
    
    // 3. Intentar diff estilo Git (- y +)
    if modified_files.is_empty() {
        let git_edits = extract_git_style_diff(output);
        for edit in &git_edits {
            if apply_single_edit_safely(workspace, edit, verbose) {
                if !modified_files.contains(&edit.filename) {
                    modified_files.push(edit.filename.clone());
                }
            }
        }
    }
    
    modified_files
}

/// Extrae edits de diff estilo Git (líneas con - y +)
fn extract_git_style_diff(output: &str) -> Vec<PendingEdit> {
    let mut edits = Vec::new();
    let mut current_file: Option<String> = None;
    let mut minus_lines: Vec<String> = Vec::new();
    let mut plus_lines: Vec<String> = Vec::new();
    let mut in_diff = false;
    
    for line in output.lines() {
        if (line.starts_with("--- a/") || line.starts_with("--- ")) 
            && !line.starts_with("---\n") {
            if let Some(ref file) = current_file {
                if !minus_lines.is_empty() || !plus_lines.is_empty() {
                    edits.push(PendingEdit {
                        filename: file.clone(),
                        search: minus_lines.join("\n"),
                        replace: plus_lines.join("\n"),
                    });
                }
            }
            let path = line.trim_start_matches("--- a/")
                          .trim_start_matches("--- ")
                          .trim();
            if !path.starts_with("/dev/null") && !path.is_empty() {
                current_file = Some(path.to_string());
            }
            minus_lines.clear();
            plus_lines.clear();
            in_diff = false;
            continue;
        }
        
        if (line.starts_with("+++ b/") || line.starts_with("+++ "))
            && !line.starts_with("+++\n") {
            let path = line.trim_start_matches("+++ b/")
                          .trim_start_matches("+++ ")
                          .trim();
            if !path.starts_with("/dev/null") && !path.is_empty() {
                current_file = Some(path.to_string());
            }
            continue;
        }
        
        if line.starts_with("@@") {
            if !minus_lines.is_empty() || !plus_lines.is_empty() {
                if let Some(ref file) = current_file {
                    edits.push(PendingEdit {
                        filename: file.clone(),
                        search: minus_lines.join("\n"),
                        replace: plus_lines.join("\n"),
                    });
                }
            }
            minus_lines.clear();
            plus_lines.clear();
            in_diff = true;
            continue;
        }
        
        if current_file.is_some() && in_diff {
            if line.starts_with('-') && !line.starts_with("---") {
                // CORREGIDO: usar skip_first_char
                minus_lines.push(skip_first_char(line).to_string());
            } else if line.starts_with('+') && !line.starts_with("+++") {
                // CORREGIDO: usar skip_first_char
                plus_lines.push(skip_first_char(line).to_string());
            } else if line.starts_with(' ') {
                // CORREGIDO: usar skip_first_char
                let ctx = skip_first_char(line).to_string();
                minus_lines.push(ctx.clone());
                plus_lines.push(ctx);
            }
        }
    }
    
    if let Some(ref file) = current_file {
        if !minus_lines.is_empty() || !plus_lines.is_empty() {
            edits.push(PendingEdit {
                filename: file.clone(),
                search: minus_lines.join("\n"),
                replace: plus_lines.join("\n"),
            });
        }
    }
    
    edits
}

/// Aplica un diff unificado
fn apply_unified_diff_edit(workspace: &Path, edit: &PendingEdit, verbose: bool) -> bool {
    let path = workspace.join(&edit.filename);
    
    if !path.exists() {
        // Archivo nuevo
        if edit.search.is_empty() && !edit.replace.is_empty() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if fs::write(&path, &edit.replace).is_ok() {
                log_ok(&format!("✓ Creado: {}", edit.filename));
                return true;
            }
        }
        return false;
    }
    
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    
    // Para diffs, intentar aplicar línea por línea
    let search = edit.search.trim();
    let replace = edit.replace.trim();
    
    if search.is_empty() {
        // Solo adiciones - agregar al final
        let new_content = format!("{}\n{}", content.trim_end(), replace);
        if fs::write(&path, new_content).is_ok() {
            log_ok(&format!("✓ Agregado a: {}", edit.filename));
            return true;
        }
    } else if content.contains(search) {
        let new_content = content.replace(search, replace);
        if new_content != content {
            if fs::write(&path, &new_content).is_ok() {
                log_ok(&format!("✓ Modificado: {}", edit.filename));
                return true;
            }
        }
    } else {
        // Intentar match fuzzy
        if let Some(matched) = find_fuzzy_match(&content, search) {
            let new_content = content.replace(&matched, replace);
            if fs::write(&path, &new_content).is_ok() {
                log_ok(&format!("✓ Modificado (fuzzy): {}", edit.filename));
                return true;
            }
        }
    }
    
    log_debug(&format!("No se pudo aplicar diff a {}", edit.filename), verbose);
    false
}

/// Detecta si hay un archivo con edición incompleta
fn detect_incomplete_file(output: &str) -> Option<PendingEdit> {
    let mut current_file: Option<String> = None;
    let mut in_search = false;
    let mut in_replace = false;
    let mut has_search = false;
    let mut search_content = String::new();
    let mut replace_content = String::new();
    
    for line in output.lines() {
        let trimmed = line.trim();
        
        if is_filename_line(trimmed) {
            current_file = Some(trimmed.to_string());
            has_search = false;
        }
        
        if line.contains("<<<<<<< SEARCH") {
            in_search = true;
            in_replace = false;
            has_search = true;
            search_content.clear();
        } else if line.contains("=======") && in_search {
            in_search = false;
            in_replace = true;
            replace_content.clear();
        } else if line.contains(">>>>>>> REPLACE") {
            in_search = false;
            in_replace = false;
            has_search = false;
        } else if in_search {
            search_content.push_str(line);
            search_content.push('\n');
        } else if in_replace {
            replace_content.push_str(line);
            replace_content.push('\n');
        }
    }
    
    // Si quedó en medio de un SEARCH/REPLACE, es incompleto
    if (in_search || in_replace || has_search) && current_file.is_some() {
        return Some(PendingEdit {
            filename: current_file.unwrap(),
            search: search_content,
            replace: replace_content,
        });
    }
    
    None
}


/// Extrae TODOS los edits posibles del output, incluso parciales
fn extract_all_possible_edits(output: &str) -> Vec<PendingEdit> {
    let mut edits = Vec::new();
    let mut current_file: Option<String> = None;
    let mut in_search = false;
    let mut in_replace = false;
    let mut search_content = String::new();
    let mut replace_content = String::new();
    
    for line in output.lines() {
        let trimmed = line.trim();
        
        // Detectar nombre de archivo
        if is_filename_line(trimmed) && !in_search && !in_replace {
            // Guardar edit anterior si existe
            if let Some(ref file) = current_file {
                if !search_content.trim().is_empty() || !replace_content.trim().is_empty() {
                    edits.push(PendingEdit {
                        filename: file.clone(),
                        search: search_content.clone(),
                        replace: replace_content.clone(),
                    });
                }
            }
            current_file = Some(trimmed.to_string());
            search_content.clear();
            replace_content.clear();
        }
        
        if line.contains("<<<<<<< SEARCH") {
            in_search = true;
            in_replace = false;
            search_content.clear();
        } else if line.contains("=======") && in_search {
            in_search = false;
            in_replace = true;
            replace_content.clear();
        } else if line.contains(">>>>>>> REPLACE") {
            if let Some(ref file) = current_file {
                if !search_content.trim().is_empty() || !replace_content.trim().is_empty() {
                    edits.push(PendingEdit {
                        filename: file.clone(),
                        search: search_content.clone(),
                        replace: replace_content.clone(),
                    });
                }
            }
            in_search = false;
            in_replace = false;
            search_content.clear();
            replace_content.clear();
        } else if in_search {
            if search_content.len() < MAX_SEARCH_REPLACE_BYTES {
                search_content.push_str(line);
                search_content.push('\n');
            }
        } else if in_replace {
            if replace_content.len() < MAX_SEARCH_REPLACE_BYTES {
                replace_content.push_str(line);
                replace_content.push('\n');
            }
        }
    }
    
    // Guardar último edit si existe (incluso si incompleto)
    if let Some(ref file) = current_file {
        if !replace_content.trim().is_empty() {
            // Solo guardar si tiene contenido de reemplazo
            edits.push(PendingEdit {
                filename: file.clone(),
                search: search_content,
                replace: replace_content,
            });
        }
    }
    
    edits
}

fn extract_unified_diff_edits_improved(output: &str) -> Vec<PendingEdit> {
    let mut edits = Vec::new();
    let mut current_file: Option<String> = None;
    let mut current_search = String::new();
    let mut current_replace = String::new();
    let mut in_hunk = false;
    
    for line in output.lines() {
        // Detectar archivo
        if line.starts_with("--- a/") || line.starts_with("--- ") {
            // Guardar anterior
            if let Some(ref file) = current_file {
                if !current_search.is_empty() || !current_replace.is_empty() {
                    edits.push(PendingEdit {
                        filename: file.clone(),
                        search: current_search.clone(),
                        replace: current_replace.clone(),
                    });
                }
            }
            
            let path = line.trim_start_matches("--- a/").trim_start_matches("--- ").trim();
            if !path.starts_with("/dev/null") {
                current_file = Some(path.to_string());
            }
            current_search.clear();
            current_replace.clear();
            in_hunk = false;
            continue;
        }
        
        if line.starts_with("+++ b/") || line.starts_with("+++ ") {
            let path = line.trim_start_matches("+++ b/").trim_start_matches("+++ ").trim();
            if !path.starts_with("/dev/null") {
                current_file = Some(path.to_string());
            }
            continue;
        }
        
        // Inicio de hunk
        if line.starts_with("@@") {
            // Guardar hunk anterior
            if !current_search.is_empty() || !current_replace.is_empty() {
                if let Some(ref file) = current_file {
                    edits.push(PendingEdit {
                        filename: file.clone(),
                        search: current_search.clone(),
                        replace: current_replace.clone(),
                    });
                }
            }
            current_search.clear();
            current_replace.clear();
            in_hunk = true;
            continue;
        }
        
        if in_hunk && current_file.is_some() {
            if line.starts_with('-') && !line.starts_with("---") {
                // Línea eliminada - va al SEARCH
                current_search.push_str(skip_first_char(line));
                current_search.push('\n');
            } else if line.starts_with('+') && !line.starts_with("+++") {
                // Línea agregada - va al REPLACE
                current_replace.push_str(skip_first_char(line));
                current_replace.push('\n');
            } else if line.starts_with(' ') {
                // Contexto - va a ambos
                let ctx = skip_first_char(line);
                current_search.push_str(ctx);
                current_search.push('\n');
                current_replace.push_str(ctx);
                current_replace.push('\n');
            }
        }
    }

    // Guardar último
    if let Some(ref file) = current_file {
        if !current_search.is_empty() || !current_replace.is_empty() {
            edits.push(PendingEdit {
                filename: file.clone(),
                search: current_search,
                replace: current_replace,
            });
        }
    }
    
    edits
}


// ═══════════════════════════════════════════════════════════════════════════════
// HELPERS ADICIONALES
// ═══════════════════════════════════════════════════════════════════════════════

use std::time::Instant;

fn is_ollama_running() -> bool {
    Command::new("pgrep")
        .args(["-x", "ollama"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn restart_ollama_with_gpu() {
    restart_ollama_with_config("default");
}

// ═══════════════════════════════════════════════════════════════════════════════
// EMERGENCY_SAVE_PROGRESS - VERSIÓN COMPLETA RESTAURADA
// ═══════════════════════════════════════════════════════════════════════════════

/// Guarda TODO el progreso posible cuando ocurre un error
fn emergency_save_progress(state: &mut AppState, output: &str) {
    log_warn("💾 ═══ GUARDADO DE EMERGENCIA ═══");
    
    let mut total_saved = 0;
    
    // ═══════════════════════════════════════════════════════════════
    // 1. FLUSH EDITS PENDIENTES DEL BUFFER
    // ═══════════════════════════════════════════════════════════════
    let flushed = state.flush_pending_edits();
    if flushed > 0 {
        log_ok(&format!("  ✓ {} edits del buffer aplicados", flushed));
        total_saved += flushed;
    }
    
    // ═══════════════════════════════════════════════════════════════
    // 2. EXTRAER Y APLICAR CUALQUIER CÓDIGO VISIBLE EN OUTPUT
    // ═══════════════════════════════════════════════════════════════
    let emergency_edits = extract_all_possible_edits(output);
    if !emergency_edits.is_empty() {
        log_info(&format!("  Encontrados {} bloques de código en output", emergency_edits.len()));
        
        let mut saved_from_output = 0;
        for edit in &emergency_edits {
            // Verificar que no sea código lazy
            if contains_lazy_pattern(&edit.replace) {
                log_debug(&format!("  Saltando edit lazy en {}", edit.filename), state.verbose);
                continue;
            }
            
            // Verificar tamaño razonable
            if edit.replace.len() > MAX_SEARCH_REPLACE_BYTES {
                log_warn(&format!("  Edit demasiado grande en {} - saltando", edit.filename));
                continue;
            }
            
            if apply_single_edit_safely(&state.workspace, edit, false) {
                saved_from_output += 1;
                total_saved += 1;
            }
        }
        
        if saved_from_output > 0 {
            log_ok(&format!("  ✓ {} edits adicionales salvados del output", saved_from_output));
        }
    }
    
    // ═══════════════════════════════════════════════════════════════
    // 3. DETECTAR ARCHIVOS INCOMPLETOS Y AGREGAR A PENDIENTES
    // ═══════════════════════════════════════════════════════════════
    if let Some(incomplete) = detect_incomplete_file(output) {
        // Crear entrada detallada para pendientes
        let incomplete_context = extract_incomplete_context(output, &incomplete.filename);
        
        let pending_entry = format!(
            "⚠️ CONTINUE EDITING: {}\n   Reason: Interrupted mid-edit\n   Last context:\n{}",
            incomplete.filename,
            incomplete_context.lines().take(10).collect::<Vec<_>>().join("\n")
        );
        
        if !state.last_next_steps.contains(&incomplete.filename) {
            state.last_next_steps = format!(
                "PRIORITY - INCOMPLETE FILES:\n{}\n\n{}",
                pending_entry,
                state.last_next_steps
            );
            log_warn(&format!("  ⚠️ Archivo incompleto detectado: {}", incomplete.filename));
        }
    }
    
    // ═══════════════════════════════════════════════════════════════
    // 4. EXTRAER NEXT_STEPS Y AGENT_HANDOFF DEL OUTPUT PARCIAL
    // ═══════════════════════════════════════════════════════════════
    if let Some(steps) = extract_next_steps(output) {
        if !steps.is_empty() && steps != state.last_next_steps {
            // Combinar con steps existentes, no reemplazar
            if !state.last_next_steps.contains(&steps) {
                state.last_next_steps = format!("{}\n\nFrom interrupted session:\n{}", 
                    state.last_next_steps, steps);
            }
            log_info("  ✓ NEXT_STEPS extraídos del output parcial");
        }
    }
    
    if let Some(handoff) = extract_agent_handoff(output) {
        if !handoff.is_empty() {
            state.last_agent_handoff = handoff;
            log_info("  ✓ AGENT_HANDOFF extraído");
        }
    }
    
    // ═══════════════════════════════════════════════════════════════
    // 5. GUARDAR TODO.MD INMEDIATAMENTE
    // ═══════════════════════════════════════════════════════════════
    state.save_todo_now();
    log_ok("  ✓ todo.md actualizado");
    
    // ═══════════════════════════════════════════════════════════════
    // 6. COMMIT DE EMERGENCIA
    // ═══════════════════════════════════════════════════════════════
    if total_saved > 0 {
        let commit_msg = format!("{}: emergency save - {} edits recovered", APP_NAME, total_saved);
        git_commit_changes(&state.workspace, &commit_msg);
        log_ok(&format!("  ✓ Commit de emergencia: {}", commit_msg));
    }
    
    // ═══════════════════════════════════════════════════════════════
    // 7. GUARDAR OUTPUT PARCIAL PARA DIAGNÓSTICO
    // ═══════════════════════════════════════════════════════════════
    if output.len() > 1000 {
        let partial_output_file = state.workspace.join(".luismind_partial_output.txt");
        if fs::write(&partial_output_file, output).is_ok() {
            log_info(&format!("  ✓ Output parcial guardado en {}", partial_output_file.display()));
        }
    }
    
    println!();
    if total_saved > 0 {
        log_ok(&format!("💾 GUARDADO COMPLETADO: {} cambios salvados", total_saved));
    } else {
        log_warn("💾 GUARDADO COMPLETADO: No había cambios pendientes");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ADD_INCOMPLETE_FILE_TO_PENDING - VERSIÓN COMPLETA RESTAURADA
// ═══════════════════════════════════════════════════════════════════════════════

/// Agrega un archivo incompleto a las tareas pendientes con contexto completo
fn add_incomplete_file_to_pending(state: &mut AppState, filename: &str, reason: &str) {
    // Crear entrada detallada
    let pending_entry = format!(
        "⚠️ [{}] INCOMPLETE - {}.\n   \
         This file must be redone with COMPLETE code.\n   \
         NEVER use '...' or 'omitted'. Show FULL implementation.",
        filename, reason
    );
    
    // Verificar que no esté ya agregado
    if !state.last_next_steps.contains(filename) {
        // Agregar como PRIORIDAD al inicio
        state.last_next_steps = format!(
            "═══ PRIORITY FIX REQUIRED ═══\n\
             {}\n\n\
             ═══ REGULAR TASKS ═══\n\
             {}",
            pending_entry,
            state.last_next_steps
        );
        
        // Guardar inmediatamente
        state.save_todo_now();
        
        log_warn(&format!("📋 Agregado a pendientes con PRIORIDAD: {}", filename));
        log_info(&format!("   Razón: {}", reason));
    } else {
        log_debug(&format!("{} ya está en pendientes", filename), state.verbose);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// CARGAR CONFIGURACIÓN EXTERNA DE OLLAMA
// ═══════════════════════════════════════════════════════════════════════════════

/// Carga configuración de Ollama desde archivo externo
fn load_ollama_config() -> HashMap<String, String> {
    let mut config = HashMap::new();
    
    // Defaults seguros
    config.insert("OLLAMA_NUM_CTX".to_string(), "32768".to_string());
    config.insert("OLLAMA_NUM_GPU".to_string(), "99".to_string());
    config.insert("OLLAMA_FLASH_ATTENTION".to_string(), "1".to_string());
    config.insert("OLLAMA_NUM_THREAD".to_string(), "16".to_string());
    config.insert("OLLAMA_NUM_BATCH".to_string(), "512".to_string());
    config.insert("OLLAMA_KEEP_ALIVE".to_string(), "24h".to_string());
    config.insert("OLLAMA_NUM_PARALLEL".to_string(), "1".to_string());
    config.insert("OLLAMA_MMAP".to_string(), "1".to_string());
    config.insert("CUDA_VISIBLE_DEVICES".to_string(), "0".to_string());
    
    // Intentar cargar archivo de configuración
    if let Some(home) = dirs::home_dir() {
        let config_path = home.join(OLLAMA_CONFIG_FILE);
        
        if config_path.exists() {
            log_info(&format!("📂 Cargando config de Ollama: {}", config_path.display()));
            
            if let Ok(content) = fs::read_to_string(&config_path) {
                for line in content.lines() {
                    let line = line.trim();
                    
                    // Ignorar comentarios y líneas vacías
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    
                    // Parsear KEY=VALUE
                    if let Some(pos) = line.find('=') {
                        let key = line[..pos].trim().to_string();
                        let value = line[pos + 1..].trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string();
                        
                        if !key.is_empty() && !value.is_empty() {
                            config.insert(key, value);
                        }
                    }
                }
            }
        }
    }
    
    // También verificar variables de entorno (override del archivo)
    for key in ["OLLAMA_NUM_CTX", "OLLAMA_NUM_GPU", "OLLAMA_FLASH_ATTENTION", 
                "OLLAMA_NUM_THREAD", "OLLAMA_NUM_BATCH", "OLLAMA_KEEP_ALIVE",
                "OLLAMA_NUM_PARALLEL", "OLLAMA_MMAP"] {
        if let Ok(val) = env::var(key) {
            config.insert(key.to_string(), val);
        }
    }
    
    config
}

/// Extrae contexto alrededor de un archivo incompleto
fn extract_incomplete_context(output: &str, filename: &str) -> String {
    let mut context = String::new();
    let mut capturing = false;
    let mut lines_captured = 0;
    
    for line in output.lines() {
        if line.contains(filename) {
            capturing = true;
        }
        
        if capturing {
            context.push_str(line);
            context.push('\n');
            lines_captured += 1;
            
            if lines_captured > 50 {
                break;
            }
        }
    }
    
    context
}
// ═══════════════════════════════════════════════════════════════════════════════
// DETECCIÓN DE NOMBRES DE ARCHIVO - ULTRA EXTENSIVO
// ═══════════════════════════════════════════════════════════════════════════════

/// Extensiones de archivo reconocidas (EXHAUSTIVO)
const FILE_EXTENSIONS: &[&str] = &[
    // ═══ Lenguajes de programación ═══
    "rs", "py", "js", "ts", "tsx", "jsx", "go", "java", "c", "cpp", "cc", "cxx",
    "h", "hpp", "hxx", "cs", "fs", "fsx", "vb", "swift", "kt", "kts", "scala",
    "clj", "cljs", "cljc", "edn", "ex", "exs", "erl", "hrl", "hs", "lhs",
    "ml", "mli", "rb", "rake", "gemspec", "php", "phtml", "php3", "php4", "php5",
    "phps", "pl", "pm", "t", "pod", "lua", "moon", "r", "R", "rmd", "jl",
    "nim", "nimble", "zig", "v", "d", "di", "ada", "adb", "ads", "pas", "pp",
    "inc", "dpr", "lpr", "cob", "cbl", "cpy", "f", "for", "f90", "f95", "f03",
    "f08", "asm", "s", "S", "m", "mm", "tcl", "tk", "exp", "ps1", "psm1",
    "psd1", "ps1xml", "bat", "cmd", "vbs", "vba", "bas", "cls", "frm",
    "awk", "sed", "dc", "factor", "forth", "fth", "4th", "io", "red", "reds",
    "rkt", "scrbl", "scm", "ss", "sld", "sls", "sps", "viml", "vim", "applescript",
    "scpt", "wl", "wls", "m", "wl", "nb", "cdf", "prolog", "pro", "P",
    "lisp", "lsp", "l", "cl", "fasl", "el", "elc", "sml", "sig", "fun",
    "ocaml", "mly", "mll", "reason", "re", "rei", "purs", "dhall", "idr",
    "agda", "lagda", "lean", "hlean", "coq", "v", "g4", "antlr", "g",
    "wat", "wast", "wasm", "sol", "vy", "yul", "move", "cairo",
    
    // ═══ Shells y scripts ═══
    "sh", "bash", "zsh", "fish", "ksh", "csh", "tcsh", "dash", "ash",
    "nu", "elvish", "ion", "xonsh", "rc", "es",
    
    // ═══ Web ═══
    "html", "htm", "xhtml", "shtml", "shtm", "hta", "htc",
    "css", "scss", "sass", "less", "styl", "stylus", "postcss",
    "vue", "svelte", "astro", "mdx", "njk", "nunjucks", "ejs",
    "hbs", "handlebars", "mustache", "pug", "jade", "haml", "slim",
    "liquid", "twig", "jinja", "jinja2", "j2", "mako", "dust",
    "asp", "aspx", "ascx", "asmx", "ashx", "master", "cshtml", "vbhtml", "razor",
    "jsp", "jspx", "jstl", "tag", "tagx",
    "erb", "rhtml",
    
    // ═══ Datos y configuración ═══
    "json", "json5", "jsonc", "jsonl", "ndjson", "geojson",
    "yaml", "yml", "eyaml",
    "toml", "tml",
    "xml", "xsd", "xsl", "xslt", "dtd", "ent", "mod", "rng", "rnc",
    "svg", "svgz", "rss", "atom", "opml", "plist", "xib", "storyboard", "nib",
    "ini", "cfg", "conf", "config", "cnf", "rc", "properties", "props",
    "env", "envrc", "env.local", "env.development", "env.production",
    "htaccess", "htpasswd", "htgroups",
    "editorconfig", "browserslistrc", "npmrc", "yarnrc", "babelrc",
    "eslintrc", "prettierrc", "stylelintrc", "huskyrc", "lintstagedrc",
    
    // ═══ Documentación ═══
    "md", "markdown", "mdown", "mkdn", "mkd", "mdwn", "mdtxt", "mdtext",
    "rst", "rest", "restx",
    "txt", "text", "log", "out",
    "adoc", "asciidoc", "asc",
    "org", "orgmode",
    "tex", "latex", "ltx", "sty", "cls", "dtx", "ins", "bib", "bst",
    "wiki", "mediawiki", "creole", "textile", "rdoc", "pod",
    "man", "1", "2", "3", "4", "5", "6", "7", "8", "9",
    "rtf", "rtfd",
    "docx", "doc", "odt", "pdf", // binarios pero a veces referenciados
    
    // ═══ Datos estructurados ═══
    "csv", "tsv", "psv", "dsv",
    "sql", "mysql", "pgsql", "plsql", "plpgsql", "sqlite", "hql", "cql",
    "graphql", "gql",
    "prisma", "dbml",
    "proto", "protobuf", "proto3",
    "thrift", "avsc", "avro", "parquet",
    "capnp", "fbs", "flatbuffers",
    
    // ═══ DevOps / IaC ═══
    "dockerfile", "containerfile",
    "tf", "tfvars", "tfstate", "hcl", "nomad", "sentinel",
    "pp", "epp", // Puppet
    "sls", "jinja", // Salt
    "j2", "jinja2",
    "cf", "cform", "template", // CloudFormation
    "bicep", "arm",
    "pulumi",
    "k8s", "kube",
    "helmfile", "values",
    "ansible", "playbook",
    "vagrantfile",
    
    // ═══ Build / Package ═══
    "make", "makefile", "mk", "mak", "gmk",
    "cmake", "cmakelists",
    "gradle", "gradlew",
    "maven", "pom",
    "sbt", "build",
    "cargo", "cabal", "stack",
    "mix", "rebar", "erlang",
    "gemfile", "rakefile", "guardfile", "brewfile", "podfile", "fastfile",
    "package", "bower", "composer", "requirements", "pipfile", "pyproject",
    "setup", "manifest", "lock", "shrinkwrap",
    "csproj", "fsproj", "vbproj", "vcxproj", "sln", "props", "targets",
    "xcodeproj", "xcworkspace", "pbxproj",
    "bazel", "build", "workspace", "bzl", "sky",
    "buck", "buckconfig",
    "pants", "build",
    "meson", "wrap",
    "ninja", "gn", "gni",
    "just", "justfile",
    "taskfile", "task",
    "dub", "dub.sdl", "dub.json",
    
    // ═══ Testing ═══
    "spec", "test", "tests", "e2e", "integration", "unit",
    "snap", "snapshot",
    "fixture", "fixtures",
    "mock", "mocks", "stub", "stubs", "fake", "fakes",
    "feature", "features", // Cucumber
    "story", "stories", // Storybook
    
    // ═══ Git ═══
    "gitignore", "gitattributes", "gitmodules", "gitconfig", "gitkeep",
    "hgignore", "hgrc", // Mercurial
    "svnignore", // SVN
    
    // ═══ IDE / Editor ═══
    "code-workspace", "sublime-project", "sublime-workspace",
    "idea", "iml", "ipr", "iws",
    "vscode", "vscodeignore",
    "project", "classpath", "factorypath",
    "launch", "tasks", "settings",
    
    // ═══ Otros ═══
    "license", "licence", "copying", "authors", "contributors", "changelog",
    "readme", "todo", "fixme", "notes", "news", "history", "changes",
    "cert", "crt", "pem", "key", "pub", "csr", "der", "p12", "pfx", "jks",
    "diff", "patch",
    "ics", "ical", "vcf", "vcard",
    "po", "pot", "mo", // i18n
    "resx", "resw", "xlf", "xliff", "strings", "stringsdict",
];

/// Detecta si una línea es un nombre de archivo
fn is_filename_line(line: &str) -> bool {
    let trimmed = line.trim();
    
    // Muy corto o muy largo = no es archivo
    if trimmed.len() < 2 || trimmed.len() > 300 {
        return false;
    }
    
    // No puede empezar con caracteres de código
    let first_char = trimmed.chars().next().unwrap_or(' ');
    if matches!(first_char, '{' | '}' | '(' | ')' | '[' | ']' | '<' | '>' | 
                            '/' | '*' | '#' | '@' | '!' | '=' | '+' | '-' |
                            ';' | ':' | '"' | '\'' | '`' | '|' | '&' | '%') {
        // Excepto si empieza con ./ o ../
        if !trimmed.starts_with("./") && !trimmed.starts_with("../") {
            return false;
        }
    }
    
    // Detectar extensión
    let has_valid_extension = if let Some(dot_pos) = trimmed.rfind('.') {
        let ext = &trimmed[dot_pos + 1..].to_lowercase();
        // Quitar posibles caracteres extra después de la extensión
        let ext_clean: String = ext.chars()
            .take_while(|c| c.is_alphanumeric())
            .collect();
        FILE_EXTENSIONS.iter().any(|e| *e == ext_clean.as_str())
    } else {
        // Archivos sin extensión conocidos
        let basename = trimmed.rsplit('/').next().unwrap_or(trimmed).to_lowercase();
        matches!(basename.as_str(), 
            "dockerfile" | "containerfile" | "makefile" | "gnumakefile" |
            "cmakelists" | "rakefile" | "gemfile" | "podfile" | "fastfile" |
            "guardfile" | "brewfile" | "vagrantfile" | "procfile" | "buildfile" |
            "justfile" | "taskfile" | "snapcraft" | "jenkinsfile" | "sonarfile" |
            "appfile" | "matchfile" | "gymfile" | "deliverfile" | "pluginfile" |
            "license" | "licence" | "copying" | "readme" | "changelog" | "authors" |
            "contributors" | "news" | "history" | "changes" | "todo" | "notes" |
            "gitignore" | "gitattributes" | "gitmodules" | "dockerignore" |
            "editorconfig" | "clang-format" | "clang-tidy" | "flake8" |
            "pylintrc" | "mypy" | "black" | "isort" | "flakeheaven" |
            "cargo" | "workspace" | "clippy"
        )
    };
    
    if !has_valid_extension {
        return false;
    }
    
    // No puede tener caracteres típicos de código en medio
    let suspicious_patterns = [
        "()", "[]", "{}", "<>", "->", "=>", "::", "==", "!=", "<=", ">=",
        "&&", "||", "++", "--", "+=", "-=", "*=", "/=", "%=",
        " = ", " + ", " - ", " * ", " / ", " % ",
        "fn ", "let ", "const ", "var ", "def ", "func ", "function ",
        "class ", "struct ", "enum ", "impl ", "trait ", "interface ",
        "import ", "from ", "use ", "require ", "include ", "export ",
        "if ", "else ", "while ", "for ", "loop ", "match ", "switch ",
        "return ", "break ", "continue ", "throw ", "raise ", "yield ",
        "pub ", "private ", "protected ", "public ", "static ", "final ",
    ];
    
    let has_suspicious = suspicious_patterns.iter()
        .any(|p| trimmed.contains(p));
    
    if has_suspicious {
        return false;
    }
    
    // Si tiene espacios, probablemente no es un archivo (excepto paths con espacios)
    // Permitir UN espacio si parece path
    let space_count = trimmed.chars().filter(|c| *c == ' ').count();
    if space_count > 2 {
        return false;
    }
    
    // Validar que parece un path válido
    let valid_path_chars = trimmed.chars().all(|c| {
        c.is_alphanumeric() || matches!(c, '/' | '\\' | '.' | '_' | '-' | ' ' | '~')
    });
    
    if !valid_path_chars {
        return false;
    }
    
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_filename() {
        // Debe detectar
        assert!(is_filename_line("src/main.rs"));
        assert!(is_filename_line("./src/lib.rs"));
        assert!(is_filename_line("../config.toml"));
        assert!(is_filename_line("package.json"));
        assert!(is_filename_line("Dockerfile"));
        assert!(is_filename_line("docker-compose.yaml"));
        assert!(is_filename_line("pom.xml"));
        assert!(is_filename_line("AndroidManifest.xml"));
        assert!(is_filename_line("styles.css"));
        assert!(is_filename_line("index.html"));
        assert!(is_filename_line("schema.graphql"));
        assert!(is_filename_line("config/database.yml"));
        assert!(is_filename_line("Makefile"));
        assert!(is_filename_line("CMakeLists.txt"));
        assert!(is_filename_line(".gitignore"));
        assert!(is_filename_line(".env.production"));
        assert!(is_filename_line("requirements.txt"));
        
        // No debe detectar (son código)
        assert!(!is_filename_line("fn main() {"));
        assert!(!is_filename_line("let x = 5;"));
        assert!(!is_filename_line("// comment"));
        assert!(!is_filename_line("use std::io;"));
        assert!(!is_filename_line("import React from 'react';"));
        assert!(!is_filename_line("<<<<<<< SEARCH"));
        assert!(!is_filename_line("======="));
        assert!(!is_filename_line(">>>>>>> REPLACE"));
        assert!(!is_filename_line("    .map(|x| x + 1)"));
        assert!(!is_filename_line("if condition && other {"));
        assert!(!is_filename_line(""));
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// CARGAR SYSTEM PROMPT DE CONFIG
// ═══════════════════════════════════════════════════════════════════════════════

/// Carga configuración de llama.cpp
fn load_llama_cpp_config() -> HashMap<String, String> {
    let mut config = HashMap::new();
    
    // Defaults
    config.insert("LLAMA_MODEL".to_string(), "".to_string());
    config.insert("LLAMA_CONTEXT".to_string(), "262144".to_string());
    config.insert("LLAMA_GPU_LAYERS".to_string(), "99".to_string());
    config.insert("LLAMA_THREADS".to_string(), "16".to_string());
    config.insert("LLAMA_BATCH".to_string(), "512".to_string());
    config.insert("LLAMA_PORT".to_string(), "8080".to_string());
    config.insert("LLAMA_HOST".to_string(), "127.0.0.1".to_string());
    config.insert("LLAMA_FLASH_ATTN".to_string(), "1".to_string());
    config.insert("LLAMA_MLOCK".to_string(), "1".to_string());
    config.insert("LLAMA_CONT_BATCHING".to_string(), "1".to_string());
    config.insert("LLAMA_CHAT_TEMPLATE".to_string(), "".to_string());
    config.insert("LLAMA_SYSTEM_PROMPT".to_string(), "".to_string());
    
    if let Some(home) = dirs::home_dir() {
        let config_path = home.join(".config/luismind/llama-cpp.conf");
        
        if config_path.exists() {
            log_info(&format!("📂 Cargando config llama.cpp: {}", config_path.display()));
            
            if let Ok(content) = fs::read_to_string(&config_path) {
                let mut current_key = String::new();
                let mut multiline_value = String::new();
                let mut in_multiline = false;
                
                for line in content.lines() {
                    let trimmed = line.trim();
                    
                    // Ignorar comentarios
                    if trimmed.starts_with('#') || trimmed.is_empty() {
                        // Si estamos en multiline, línea vacía podría ser parte del valor
                        if in_multiline && !trimmed.starts_with('#') {
                            multiline_value.push('\n');
                        }
                        continue;
                    }
                    
                    if let Some(pos) = trimmed.find('=') {
                        // Guardar valor multiline anterior si existe
                        if in_multiline && !current_key.is_empty() {
                            config.insert(current_key.clone(), multiline_value.trim().to_string());
                        }
                        
                        let key = trimmed[..pos].trim().to_string();
                        let value = trimmed[pos + 1..].trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string();
                        
                        // System prompt puede ser multiline
                        if key == "LLAMA_SYSTEM_PROMPT" {
                            current_key = key;
                            multiline_value = value;
                            in_multiline = true;
                        } else {
                            config.insert(key, value);
                            in_multiline = false;
                        }
                    } else if in_multiline {
                        // Continuar acumulando valor multiline
                        multiline_value.push('\n');
                        multiline_value.push_str(trimmed);
                    }
                }
                
                // Guardar último valor multiline
                if in_multiline && !current_key.is_empty() {
                    config.insert(current_key, multiline_value.trim().to_string());
                }
            }
        }
    }
    
    config
}


/// Inicia llama.cpp server con configuración
fn start_llama_cpp_server() -> bool {
    let config = load_llama_cpp_config();
    
    let model = config.get("LLAMA_MODEL").map(|s| s.as_str()).unwrap_or("");
    if model.is_empty() {
        log_error("LLAMA_MODEL no configurado en ~/.config/luismind/llama-cpp.conf");
        return false;
    }
    
    let context: i64 = config.get("LLAMA_CONTEXT")
        .and_then(|s| s.parse().ok())
        .unwrap_or(262144);
    
    let gpu_layers: i64 = config.get("LLAMA_GPU_LAYERS")
        .and_then(|s| s.parse().ok())
        .unwrap_or(99);
    
    let threads: i64 = config.get("LLAMA_THREADS")
        .and_then(|s| s.parse().ok())
        .unwrap_or(16);
        
    let batch: i64 = config.get("LLAMA_BATCH")
        .and_then(|s| s.parse().ok())
        .unwrap_or(512);
    
    let port = config.get("LLAMA_PORT")
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    
    let host = config.get("LLAMA_HOST")
        .map(|s| s.as_str())
        .unwrap_or("127.0.0.1");
    
    let use_flash = config.get("LLAMA_FLASH_ATTN")
        .map(|s| s == "1")
        .unwrap_or(true);
    
    let use_mlock = config.get("LLAMA_MLOCK")
        .map(|s| s == "1")
        .unwrap_or(true);
    
    log_info("═══ INICIANDO LLAMA.CPP SERVER ═══");
    log_info(&format!("  Modelo: {}", model));
    log_info(&format!("  Contexto: {}k tokens", context / 1024));
    log_info(&format!("  GPU layers: {}", gpu_layers));
    log_info(&format!("  Threads: {}", threads));
    log_info(&format!("  Puerto: {}", port));
    
    let mut cmd = Command::new("llama-server");
    
    cmd.args(["-m", model])
       .args(["-c", &context.to_string()])
       .args(["-ngl", &gpu_layers.to_string()])
       .args(["-t", &threads.to_string()])
       .args(["-b", &batch.to_string()])
       .args(["--host", host])
       .args(["--port", &port.to_string()]);
    
    if use_flash {
        cmd.arg("-fa");
    }
    
    if use_mlock {
        cmd.arg("--mlock");
    }
    
    // Continuous batching para mejor throughput
    if config.get("LLAMA_CONT_BATCHING").map(|s| s == "1").unwrap_or(true) {
        cmd.arg("-cb");
    }
    
    cmd.stdout(Stdio::null())
       .stderr(Stdio::null());
    
    match cmd.spawn() {
        Ok(_) => {
            log_info("Esperando a que llama.cpp server esté listo...");
            for i in 0..60 {
                thread::sleep(Duration::from_secs(1));
                if is_llama_cpp_running(port) {
                    log_ok(&format!("✓ llama.cpp server listo en {}s", i + 1));
                    return true;
                }
            }
            log_error("llama.cpp server no responde después de 60s");
            false
        }
        Err(e) => {
            log_error(&format!("Error iniciando llama.cpp server: {}", e));
            log_info("¿Instalaste llama.cpp? yay -S llama.cpp-cuda");
            false
        }
    }
}

/// Verifica si llama.cpp server está corriendo
fn is_llama_cpp_running(port: u16) -> bool {
    use std::net::TcpStream;
    TcpStream::connect(format!("127.0.0.1:{}", port))
        .map(|_| true)
        .unwrap_or(false)
}

/// Obtiene URL base para API según proveedor
fn get_api_base_url(state: &AppState, use_llama_cpp: bool, llama_port: u16) -> Option<String> {
    if use_llama_cpp {
        Some(format!("http://127.0.0.1:{}/v1", llama_port))
    } else if state.model.provider == "ollama" {
        Some("http://127.0.0.1:11434/v1".to_string())
    } else {
        None  // Usar default del proveedor
    }
}
// ═══════════════════════════════════════════════════════════════════════════════
// OOBABOOGA / TEXT-GENERATION-WEBUI / APIS LOCALES
// ═══════════════════════════════════════════════════════════════════════════════

/// Verifica si una API está corriendo en host:port
fn is_local_api_running(host: &str, port: u16) -> bool {
    use std::net::TcpStream;
    use std::time::Duration;
    
    let addr = format!("{}:{}", host, port);
    TcpStream::connect_timeout(
        //&addr.parse().unwrap_or_else(|_| format!("{}:{}", host, port).parse().unwrap()),
        &addr.parse().unwrap_or_else(|_| format!("127.0.0.1:{}", port).parse().unwrap()),
        Duration::from_secs(2)
    ).is_ok()
}
/// Verifica si una API está corriendo usando su config
fn is_api_running(config: &LocalApiConfig) -> bool {
    is_local_api_running(&config.host, config.port)
}


/// Obtiene el modelo actualmente cargado (para oobabooga)
fn get_current_model_from_api(config: &LocalApiConfig) -> Option<String> {
    let url = format!("{}/internal/model/info", config.get_api_base());
    
    let output = Command::new("curl")
        .args(["-s", "-m", "5", &url])
        .output()
        .ok()?;
    
    if !output.status.success() {
        return None;
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Buscar "model_name": "valor"
    if let Some(pos) = stdout.find("\"model_name\"") {
        let after = &stdout[pos + 12..];
        if let Some(colon) = after.find(':') {
            let value_part = &after[colon + 1..];
            if let Some(q1) = value_part.find('"') {
                let after_q1 = &value_part[q1 + 1..];
                if let Some(q2) = after_q1.find('"') {
                    return Some(after_q1[..q2].to_string());
                }
            }
        }
    }
    
    None
}

/// Obtiene modelos de una API OpenAI-compatible usando curl
fn fetch_openai_compatible_models(config: &LocalApiConfig) -> Vec<DynamicModel> {
    let api_base = config.get_api_base();
    let url = format!("{}/models", api_base.trim_end_matches('/'));
    
    // Construir comando curl
    let mut cmd = Command::new("curl");
    cmd.args(["-s", "-m", "10", &url]);
    
    // Agregar header de auth si hay API key
    if !config.api_key.is_empty() && config.api_key != "sk-dummy" {
        cmd.args(["-H", &format!("Authorization: Bearer {}", config.api_key)]);
    }
    
    let output = match cmd.output() {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_models_json(&stdout, &config.provider, config.default_context)
}

/// Parsea JSON de modelos manualmente
fn parse_models_json(json: &str, provider_name: &str, default_context: usize) -> Vec<DynamicModel> {
    let mut models = Vec::new();
    let mut current_pos = 0;
    
    // Buscar "id": "nombre" en el JSON
    while let Some(pos) = json[current_pos..].find("\"id\"") {
        let abs_pos = current_pos + pos;
        current_pos = abs_pos + 4;
        
        if let Some(colon_pos) = json[current_pos..].find(':') {
            let value_start = current_pos + colon_pos + 1;
            
            if let Some(quote1) = json[value_start..].find('"') {
                let name_start = value_start + quote1 + 1;
                
                if let Some(quote2) = json[name_start..].find('"') {
                    let model_name = &json[name_start..name_start + quote2];
                    
                    if !model_name.is_empty() 
                        && !model_name.contains('{') 
                        && !model_name.contains('}')
                        && model_name.len() < 200 
                    {
                        let context = infer_context_from_model_name(model_name)
                            .max(default_context);
                        
                        models.push(DynamicModel {
                            name: model_name.to_string(),
                            display_name: format!("{} ({})", model_name, provider_name),
                            provider: provider_name.to_string(),
                            token_limit: context,
                            is_free: true,
                        });
                    }
                    
                    current_pos = name_start + quote2 + 1;
                }
            }
        }
    }
    
    models
}

/// Obtiene el modelo actualmente cargado en oobabooga
fn get_oobabooga_current_model(port: u16) -> Option<String> {
    let url = format!("http://127.0.0.1:{}/v1/internal/model/info", port);
    
    let output = Command::new("curl")
        .args(["-s", "-m", "5", &url])
        .output()
        .ok()?;
    
    if !output.status.success() {
        return None;
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Buscar "model_name": "valor"
    if let Some(pos) = stdout.find("\"model_name\"") {
        let after = &stdout[pos + 12..];
        if let Some(colon) = after.find(':') {
            let value_part = &after[colon + 1..];
            if let Some(q1) = value_part.find('"') {
                let after_q1 = &value_part[q1 + 1..];
                if let Some(q2) = after_q1.find('"') {
                    return Some(after_q1[..q2].to_string());
                }
            }
        }
    }
    
    None
}

fn get_all_local_api_models() -> Vec<DynamicModel> {
    let mut all_models = Vec::new();
    let configs = load_local_apis_config();
    
    for cfg in configs {
        if !cfg.enabled {
            continue;
        }
        
        if !is_local_api_running(&cfg.host, cfg.port) {
            continue;
        }
        
        log_info(&format!("📡 Detectado {} en {}:{}...", cfg.name, cfg.host, cfg.port));
        
        // Usar cfg.name como prefix si cfg.prefix está vacío
        let prefix = if cfg.prefix.is_empty() { &cfg.name } else { &cfg.prefix };
        let aider_prefix = if cfg.aider_model_prefix.is_empty() { 
            "openai/".to_string() 
        } else { 
            cfg.aider_model_prefix.clone() 
        };
        
        // Si hay modelos configurados manualmente
        if !cfg.models.is_empty() {
            for m in &cfg.models {
                all_models.push(DynamicModel {
                    name: format!("{}/{}", prefix, m.name),
                    id: format!("{}{}", aider_prefix, m.name),
                    display_name: format!("{} ({})", m.display_name, cfg.name),
                    provider: cfg.provider.clone(),
                    token_limit: m.context,
                    is_free: true,
                    cooldown_time: 0,
                });
            }
            log_ok(&format!("  {} modelos configurados", cfg.models.len()));
        } else {
            // Fetch dinámico
            let api_base = if cfg.api_base.is_empty() {
                format!("http://{}:{}/v1", cfg.host, cfg.port)
            } else {
                cfg.api_base.clone()
            };
            
            let models = parse_models_from_json_local(&api_base, &cfg.provider, &aider_prefix, prefix, cfg.default_context);
            
            if !models.is_empty() {
                log_ok(&format!("  {} modelos detectados", models.len()));
                all_models.extend(models);
            } else {
                // Modelo genérico
                all_models.push(DynamicModel {
                    name: format!("{}/local", prefix),
                    id: format!("{}local", aider_prefix),
                    display_name: format!("Local ({})", cfg.name),
                    provider: cfg.provider.clone(),
                    token_limit: cfg.default_context,
                    is_free: true,
                    cooldown_time: 0,
                });
            }
        }
    }
    
    all_models
}

/// Fetch modelos de API local
fn parse_models_from_json_local(api_base: &str, provider: &str, aider_prefix: &str, user_prefix: &str, default_context: usize) -> Vec<DynamicModel> {
    let url = format!("{}/models", api_base.trim_end_matches('/'));
    
    let output = Command::new("curl")
        .args(["-s", "-m", "10", &url])
        .output();
    
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    
    let json = String::from_utf8_lossy(&output.stdout);
    parse_models_from_json(&json, provider, aider_prefix, user_prefix, default_context, 0)
}

/// Fetch de modelos desde API local OpenAI-compatible
fn fetch_openai_compatible_local(api_base: &str, cfg: &LocalApiConfig) -> Vec<DynamicModel> {
    let url = format!("{}/models", api_base.trim_end_matches('/'));
    
    let output = Command::new("curl")
        .args(["-s", "-m", "10", &url])
        .output();
    
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    
    let json = String::from_utf8_lossy(&output.stdout);
    parse_models_from_json(
        &json,
        &cfg.provider,
        &cfg.aider_model_prefix,
        &cfg.prefix,
        cfg.default_context,
        0,  // Local = sin cooldown
    )
}

/// Encuentra config de una API local por provider
fn find_local_api_config(provider: &str) -> Option<LocalApiConfig> {
    let configs = load_local_apis_config();
    configs.into_iter().find(|c| c.provider == provider || c.name == provider)
}

// ═══════════════════════════════════════════════════════════════════════════════
// FUNCIONES ÚNICAS (ELIMINAR DUPLICADOS)
// ═══════════════════════════════════════════════════════════════════════════════

/// Verifica que un modelo de Ollama existe - ÚNICA DEFINICIÓN
fn ensure_ollama_model(model_name: &str) -> bool {
    let output = Command::new("ollama")
        .args(["list"])
        .output();
    
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let name_lower = model_name.to_lowercase();
    
    stdout.lines().skip(1).any(|line| {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let Some(first) = parts.first() {
            let model_lower = first.to_lowercase();
            model_lower == name_lower || 
            model_lower == format!("{}:latest", name_lower) ||
            model_lower.contains(&name_lower)
        } else {
            false
        }
    })
}

/// Espera con countdown visual - ÚNICA DEFINICIÓN
fn wait_with_countdown(seconds: u64, reason: &str) {
    if seconds == 0 {
        return;
    }
    
    if !reason.is_empty() {
        log_wait(&format!("Esperando {}s {}...", seconds, reason));
    } else {
        log_wait(&format!("Esperando {}s...", seconds));
    }
    
    for remaining in (1..=seconds).rev() {
        if should_exit() {
            return;
        }
        
        if remaining % 10 == 0 || remaining <= 5 {
            print!("\r⏳ {}s restantes...    ", remaining);
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        
        thread::sleep(Duration::from_secs(1));
    }
    
    println!();
}

// ═══════════════════════════════════════════════════════════════════════════════
// ALIASES PARA COMPATIBILIDAD
// ═══════════════════════════════════════════════════════════════════════════════

/// Alias: find_model_by_name -> find_model_unified
fn find_model_by_name(query: &str, models: &[DynamicModel]) -> Option<DynamicModel> {
    find_model_unified(query, models)
}

/// Alias: find_api_for_model
fn find_api_for_model(model: &DynamicModel) -> Option<ApiConfig> {
    find_api_config_for_provider(&model.provider)
}

/// Alias: handle_api_error_with_wait (para código existente)
fn handle_api_error_with_wait(state: &mut AppState, err: &ApiError) -> bool {
    // Obtener config del provider actual
    let mut api_config = find_api_config_for_provider(&state.model.provider)
        .unwrap_or_default();
    
    handle_api_error_with_cooldown(state, err, &mut api_config)
}

/// Alias: configure_aider_command
impl ApiConfig {
    fn configure_aider_command(&self, cmd: &mut Command, keys: &HashMap<String, String>) {
        self.configure_aider(cmd, keys);
    }
}





// ═══════════════════════════════════════════════════════════════════════════════
// FETCH DE MODELOS - UNIFICADO
// ═══════════════════════════════════════════════════════════════════════════════

/// Obtiene modelos de Gemini
fn fetch_gemini_models(api_key: &str) -> Vec<DynamicModel> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models?key={}",
        api_key
    );
    
    let output = Command::new("curl")
        .args(["-s", "-m", "15", &url])
        .output();
    
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    
    let json = String::from_utf8_lossy(&output.stdout);
    parse_models_from_json(&json, "gemini", "gemini/", "gemini", 1_000_000, 30)
}

/// Obtiene modelos de Groq
fn fetch_groq_models(api_key: Option<&str>) -> Vec<DynamicModel> {
    let key = api_key
        .map(|s| s.to_string())
        .or_else(|| std::env::var("GROQ_API_KEY").ok())
        .unwrap_or_default();
    
    if key.is_empty() {
        return Vec::new();
    }
    
    let output = Command::new("curl")
        .args([
            "-s", "-m", "15",
            "-H", &format!("Authorization: Bearer {}", key),
            "https://api.groq.com/openai/v1/models"
        ])
        .output();
    
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    
    let json = String::from_utf8_lossy(&output.stdout);
    parse_models_from_json(&json, "groq", "groq/", "groq", 131_072, 30)
}

/// Obtiene modelos de DeepSeek
fn fetch_deepseek_models(api_key: Option<&str>) -> Vec<DynamicModel> {
    let key = api_key
        .map(|s| s.to_string())
        .or_else(|| std::env::var("DEEPSEEK_API_KEY").ok())
        .unwrap_or_default();
    
    if key.is_empty() {
        return Vec::new();
    }
    
    let output = Command::new("curl")
        .args([
            "-s", "-m", "15",
            "-H", &format!("Authorization: Bearer {}", key),
            "https://api.deepseek.com/v1/models"
        ])
        .output();
    
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    
    let json = String::from_utf8_lossy(&output.stdout);
    parse_models_from_json(&json, "deepseek", "deepseek/", "deepseek", 65_536, 60)
}

// ═══════════════════════════════════════════════════════════════════════════════
// PARSER UNIFICADO DE JSON
// ═══════════════════════════════════════════════════════════════════════════════

/// Parsea JSON de modelos y crea DynamicModels completos
fn parse_models_from_json(
    json: &str,
    provider: &str,
    aider_prefix: &str,
    user_prefix: &str,
    default_context: usize,
    cooldown: u64,
) -> Vec<DynamicModel> {
    let mut models = Vec::new();
    
    // Detectar formato (Gemini usa "models/name", otros usan "id")
    let is_gemini = provider == "gemini";
    let id_field = if is_gemini { "name" } else { "id" };
    
    let mut pos = 0;
    while let Some(field_pos) = json[pos..].find(&format!("\"{}\"", id_field)) {
        let abs_pos = pos + field_pos;
        pos = abs_pos + id_field.len() + 2;
        
        // Buscar el valor
        if let Some(colon) = json[pos..].find(':') {
            let value_start = pos + colon + 1;
            
            if let Some(q1) = json[value_start..].find('"') {
                let name_start = value_start + q1 + 1;
                
                if let Some(q2) = json[name_start..].find('"') {
                    let raw_value = &json[name_start..name_start + q2];
                    
                    // Limpiar el nombre
                    let raw_name = if is_gemini && raw_value.starts_with("models/") {
                        &raw_value[7..]  // Quitar "models/"
                    } else {
                        raw_value
                    };
                    
                    // Validar
                    if raw_name.is_empty() || raw_name.len() > 200 ||
                       raw_name.contains('{') || raw_name.contains('}') {
                        continue;
                    }
                    
                    // Filtrar no relevantes
                    if should_skip_model(raw_name) {
                        continue;
                    }
                    
                    // Construir modelo completo
                    let user_name = format!("{}/{}", user_prefix, raw_name);
                    let aider_id = format!("{}{}", aider_prefix, raw_name);
                    let context = infer_context_from_model_name(raw_name).max(default_context);
                    let is_free = cooldown == 0;  // Local = free
                    
                    let model = DynamicModel {
                        name: user_name.clone(),
                        id: aider_id,
                        display_name: format!("{} ({})", raw_name, provider),
                        provider: provider.to_string(),
                        token_limit: context,
                        is_free,
                        cooldown_time: cooldown,
                    };
                    
                    // Evitar duplicados
                    if !models.iter().any(|m: &DynamicModel| m.name == user_name) {
                        models.push(model);
                    }
                    
                    pos = name_start + q2 + 1;
                }
            }
        }
    }
    
    models
}

/// Determina si un modelo debe saltarse
fn should_skip_model(name: &str) -> bool {
    let lower = name.to_lowercase();
    
    // Embedding
    if lower.contains("embed") && !lower.contains("code") {
        return true;
    }
    
    // Audio/Image
    if lower.contains("whisper") || lower.contains("tts") || 
       lower.contains("dall-e") || lower.contains("imagen") {
        return true;
    }
    
    // Moderation
    if lower.contains("moderation") {
        return true;
    }
    
    // Muy viejos
    if (lower.contains("davinci") || lower.contains("curie") || 
        lower.contains("babbage") || lower.contains("ada")) && !lower.contains("gpt") {
        return true;
    }
    
    false
}