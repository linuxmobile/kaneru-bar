use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppInfo {
    pub desktop_id: String,
    pub name: String,
    pub icon: String,
    pub exec: String,
    pub generic_name: Option<String>,
    pub keywords: Vec<String>,
}

impl AppResolver {
    pub fn resolve_by_desktop_id(&self, desktop_id: &str) -> Option<&AppInfo> {
        self.apps_by_desktop_id.get(&desktop_id.to_lowercase())
    }

    pub fn clean_exec(exec: &str) -> Vec<String> {
        exec.split_whitespace()
            .filter(|part| {
                !part.starts_with('%')
            })
            .map(|s| s.to_string())
            .collect()
    }
}

pub struct AppResolver {
    apps_by_name: HashMap<String, AppInfo>,
    apps_by_exec: HashMap<String, AppInfo>,
    apps_by_desktop_id: HashMap<String, AppInfo>,
}

impl AppResolver {
    pub fn new() -> Self {
        let mut resolver = Self {
            apps_by_name: HashMap::new(),
            apps_by_exec: HashMap::new(),
            apps_by_desktop_id: HashMap::new(),
        };
        
        resolver.scan_all_applications();
        resolver
    }
    
    fn scan_all_applications(&mut self) {
        let search_paths = self.get_desktop_file_paths();
        
        for path in search_paths {
            if path.exists() {
                self.scan_directory(&path);
            }
        }
    }
    
    fn get_desktop_file_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        ];
        
        if let Some(home) = std::env::var_os("HOME") {
            let home_path = PathBuf::from(home);
            paths.push(home_path.join(".local/share/applications"));
            paths.push(home_path.join(".local/share/flatpak/exports/share/applications"));
        }
        
        if let Some(xdg_data_dirs) = std::env::var_os("XDG_DATA_DIRS") {
            for dir in std::env::split_paths(&xdg_data_dirs) {
                paths.push(dir.join("applications"));
            }
        }
        
        paths
    }
    
    fn scan_directory(&mut self, dir: &PathBuf) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
                    if let Some(app_info) = self.parse_desktop_file(&path) {
                        self.register_app(app_info);
                    }
                }
            }
        }
    }
    
    fn parse_desktop_file(&self, path: &PathBuf) -> Option<AppInfo> {
        let content = fs::read_to_string(path).ok()?;
        let desktop_id = path.file_stem()?.to_str()?.to_string();
        
        let mut name = None;
        let mut icon = None;
        let mut exec = None;
        let mut generic_name = None;
        let mut keywords = Vec::new();
        let mut no_display = false;
        let mut app_type = None;
        let mut in_desktop_entry = false;
        
        for line in content.lines() {
            let line = line.trim();
            
            if line == "[Desktop Entry]" {
                in_desktop_entry = true;
                continue;
            } else if line.starts_with('[') && line.ends_with(']') {
                in_desktop_entry = false;
                continue;
            }
            
            if !in_desktop_entry {
                continue;
            }
            
            if line.starts_with("Name=") && !line.contains('[') {
                name = Some(line[5..].to_string());
            } else if line.starts_with("Icon=") {
                let icon_value = line[5..].to_string();
                if !icon_value.is_empty() {
                    icon = Some(icon_value);
                }
            } else if line.starts_with("Exec=") {
                let exec_value = line[5..].to_string();
                let clean_exec = exec_value
                    .replace("%u", "")
                    .replace("%U", "")
                    .replace("%f", "")
                    .replace("%F", "")
                    .trim()
                    .to_string();
                exec = Some(clean_exec);
            } else if line.starts_with("GenericName=") && !line.contains('[') {
                generic_name = Some(line[12..].to_string());
            } else if line.starts_with("Keywords=") {
                keywords = line[9..].split(';').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
            } else if line == "NoDisplay=true" {
                no_display = true;
            } else if line.starts_with("Type=") {
                app_type = Some(line[5..].to_string());
            }
        }
        
        if no_display || app_type.as_deref() != Some("Application") {
            return None;
        }
        
        let name = name?;
        let icon = icon.unwrap_or_else(|| "application-x-executable".to_string());
        let exec = exec?;
        
        Some(AppInfo {
            desktop_id,
            name,
            icon,
            exec,
            generic_name,
            keywords,
        })
    }
    
    fn register_app(&mut self, app_info: AppInfo) {
        let name_key = app_info.name.to_lowercase();
        let exec_key = self.extract_command_name(&app_info.exec).to_lowercase();
        let desktop_key = app_info.desktop_id.to_lowercase();
        
        self.apps_by_name.insert(name_key, app_info.clone());
        self.apps_by_exec.insert(exec_key, app_info.clone());
        self.apps_by_desktop_id.insert(desktop_key, app_info.clone());
        
        if let Some(generic) = &app_info.generic_name {
            let generic_key = generic.to_lowercase();
            if !self.apps_by_name.contains_key(&generic_key) {
                self.apps_by_name.insert(generic_key, app_info.clone());
            }
        }
        
        for keyword in &app_info.keywords {
            if !self.apps_by_name.contains_key(keyword) {
                self.apps_by_name.insert(keyword.clone(), app_info.clone());
            }
        }
    }
    
    pub fn extract_command_name(&self, exec: &str) -> String {
        let clean_exec = exec
            .replace("%u", "")
            .replace("%U", "")
            .replace("%f", "")
            .replace("%F", "")
            .replace("--new-window", "")
            .replace("--incognito", "")
            .replace("--private-window", "");
            
        let trimmed = clean_exec.trim();
        let command = trimmed
            .split_whitespace()
            .next()
            .unwrap_or(trimmed);
            
        command
            .split('/')
            .last()
            .unwrap_or(command)
            .to_string()
    }
    
    pub fn resolve(&self, query: &str) -> Option<&AppInfo> {
        let query_lower = query.to_lowercase();
        
        if let Some(app) = self.apps_by_name.get(&query_lower) {
            return Some(app);
        }
        
        if let Some(app) = self.apps_by_exec.get(&query_lower) {
            return Some(app);
        }
        
        if let Some(app) = self.apps_by_desktop_id.get(&query_lower) {
            return Some(app);
        }
        
        self.fuzzy_search(&query_lower)
    }
    
    fn fuzzy_search(&self, query: &str) -> Option<&AppInfo> {
        let mut best_match = None;
        let mut best_score = 0;
        
        for (_, app) in &self.apps_by_desktop_id {
            let mut score = 0;
            
            score = score.max(self.calculate_match_score(query, &app.name.to_lowercase(), &app.name));
            score = score.max(self.calculate_match_score(query, &app.desktop_id.to_lowercase(), &app.desktop_id));
            score = score.max(self.calculate_match_score(query, &self.extract_command_name(&app.exec).to_lowercase(), &app.exec));
            
            if let Some(generic) = &app.generic_name {
                score = score.max(self.calculate_match_score(query, &generic.to_lowercase(), generic));
            }
            
            for keyword in &app.keywords {
                score = score.max(self.calculate_match_score(query, keyword, keyword));
            }
            
            if score > best_score {
                best_score = score;
                best_match = Some(app);
            }
        }
        
        if best_score > 30 {
            best_match
        } else {
            None
        }
    }
    
    fn calculate_match_score(&self, query: &str, key: &str, display_name: &str) -> i32 {
        if key == query {
            return 100;
        }
        
        if key.starts_with(query) {
            return 90;
        }
        
        if key.contains(query) {
            return 70;
        }
        
        if display_name.to_lowercase().contains(query) {
            return 60;
        }
        
        let query_normalized = query.replace("-", "").replace("_", "").replace(" ", "");
        let key_normalized = key.replace("-", "").replace("_", "").replace(" ", "");
        let display_normalized = display_name.to_lowercase().replace("-", "").replace("_", "").replace(" ", "");
        
        if key_normalized.contains(&query_normalized) {
            return 65;
        }
        
        if display_normalized.contains(&query_normalized) {
            return 55;
        }
        
        let query_words: Vec<&str> = query.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect();
        let key_words: Vec<&str> = key.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect();
        let display_words: Vec<&str> = display_name.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect();
        
        let mut matches = 0;
        let total_words = query_words.len();
        
        for q_word in &query_words {
            for k_word in &key_words {
                if k_word.to_lowercase().starts_with(&q_word.to_lowercase()) {
                    matches += 1;
                    break;
                }
            }
        }
        
        if matches == 0 {
            for q_word in &query_words {
                for d_word in &display_words {
                    if d_word.to_lowercase().starts_with(&q_word.to_lowercase()) {
                        matches += 1;
                        break;
                    }
                }
            }
        }
        
        if matches > 0 {
            (matches * 50) / total_words as i32
        } else {
            0
        }
    }
    

}

impl Default for AppResolver {
    fn default() -> Self {
        Self::new()
    }
}