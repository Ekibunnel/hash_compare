use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone, Debug)]
struct HashEntry {
    hash: String,
    filepath: String,
}

#[derive(Clone, Debug)]
enum ComparisonResult {
    FullMatch(String),
    PartialMatch(String, String),
    Mismatch(String, String),
    OnlyInFile1(String),
    OnlyInFile2(String),
}

struct ComparisonStats {
    full_matches: usize,
    partial_matches: usize,
    mismatches: usize,
    only_in_file1: usize,
    only_in_file2: usize,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <file1> <file2> [output_dir]", args[0]);
        std::process::exit(1);
    }

    let file1 = &args[1];
    let file2 = &args[2];
    let output_dir = args.get(3).map(|s| s.as_str());
    let show_results = output_dir.is_none();

    println!("Reading files...");
    let hashes1 = read_hashes(file1);
    let hashes2 = read_hashes(file2);

    if hashes1.is_empty() || hashes2.is_empty() {
        eprintln!("Error reading files or files are empty");
        std::process::exit(1);
    }

    println!(
        "File1: {} entries, File2: {} entries\n",
        hashes1.len(),
        hashes2.len()
    );

    let results = Arc::new(Mutex::new(Vec::new()));
    let processed_count = Arc::new(Mutex::new(0usize));
    let total_count = hashes1.len();
    let mut handles = vec![];

    let num_threads = num_cpus::get();
    let chunk_size = (hashes1.len() + num_threads - 1) / num_threads;

    let hashes1_vec: Vec<HashEntry> = hashes1.into_iter().collect();
    let hashes2_arc = Arc::new(hashes2);

    for chunk_start in (0..hashes1_vec.len()).step_by(chunk_size) {
        let chunk_end = std::cmp::min(chunk_start + chunk_size, hashes1_vec.len());
        let chunk = hashes1_vec[chunk_start..chunk_end].to_vec();
        let hashes2_clone = Arc::clone(&hashes2_arc);
        let results_clone = Arc::clone(&results);
        let processed_clone = Arc::clone(&processed_count);

        let handle = thread::spawn(move || {
            let local_results = compare_chunk(&chunk, &hashes2_clone);
            let mut results_guard = results_clone.lock().unwrap();
            results_guard.extend(local_results);

            let mut processed = processed_clone.lock().unwrap();
            *processed += chunk.len();
        });

        handles.push(handle);
    }

    let processed_clone = Arc::clone(&processed_count);
    let progress_handle = if !show_results {
        Some(thread::spawn(move || {
            while *processed_clone.lock().unwrap() < total_count {
                let current = *processed_clone.lock().unwrap();
                print_progress_bar(current, total_count);
                thread::sleep(std::time::Duration::from_millis(100));
            }
            print_progress_bar(total_count, total_count);
            println!();
        }))
    } else {
        None
    };

    for handle in handles {
        handle.join().unwrap();
    }

    if let Some(handle) = progress_handle {
        handle.join().unwrap();
    }

    let results_clone = Arc::clone(&results);
    let hashes2_clone = Arc::clone(&hashes2_arc);
    let hashes1_arc = Arc::new(hashes1_vec);

    let handle = thread::spawn(move || {
        let local_results = find_only_in_file2(&hashes1_arc, &hashes2_clone);
        let mut results_guard = results_clone.lock().unwrap();
        results_guard.extend(local_results);
    });
    handle.join().unwrap();

    let final_results = results.lock().unwrap();

    if show_results {
        display_results(&final_results);
    }

    if let Some(out_dir) = output_dir {
        match export_to_csv(&final_results, out_dir, file1, file2) {
            Ok(_) => println!("Results exported to: {}", out_dir),
            Err(e) => eprintln!("Error exporting results: {}", e),
        }
    }
}

fn print_progress_bar(current: usize, total: usize) {
    let bar_length = 50;
    let percentage = if total > 0 {
        (current as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    let filled = if total > 0 {
        (current as f64 / total as f64 * bar_length as f64) as usize
    } else {
        0
    };

    print!("\rProgress: [");
    for i in 0..bar_length {
        if i < filled {
            print!("=");
        } else {
            print!(" ");
        }
    }
    print!("] {:.1}% ({}/{})", percentage, current, total);
    std::io::stdout().flush().unwrap();
}

fn read_hashes(filename: &str) -> Vec<HashEntry> {
    let mut hashes = Vec::new();

    let path = Path::new(filename);
    let file = match File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error opening file '{}': {}", filename, e);
            return hashes;
        }
    };

    let reader = BufReader::new(file);

    for line in reader.lines() {
        if let Ok(line) = line {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, " *").collect();
            if parts.len() == 2 {
                let hash = parts[0].trim().to_string();
                let filepath = parts[1].trim().to_string();

                hashes.push(HashEntry { hash, filepath });
            } else {
                eprintln!("Warning: Invalid line format (missing ' *'): {}", line);
            }
        }
    }

    hashes
}

fn compare_chunk(
    chunk: &[HashEntry],
    hashes2: &Vec<HashEntry>,
) -> Vec<(String, ComparisonResult)> {
    let mut results = Vec::new();

    for entry1 in chunk {
        let matching_entries: Vec<&HashEntry> =
            hashes2.iter().filter(|e| e.filepath == entry1.filepath).collect();

        if !matching_entries.is_empty() {
            for entry2 in matching_entries {
                let result = compare_hashes(&entry1.hash, &entry2.hash);
                results.push((entry1.filepath.clone(), result));
            }
        } else {
            results.push((
                entry1.filepath.clone(),
                ComparisonResult::OnlyInFile1(entry1.hash.clone()),
            ));
        }
    }

    results
}

fn compare_hashes(hash1: &str, hash2: &str) -> ComparisonResult {
    if hash1 == hash2 {
        return ComparisonResult::FullMatch(hash1.to_string());
    }

    if hash1.contains("***") {
        if is_partial_match(hash1, hash2) {
            return ComparisonResult::PartialMatch(hash1.to_string(), hash2.to_string());
        }
    }

    if hash2.contains("***") {
        if is_partial_match(hash2, hash1) {
            return ComparisonResult::PartialMatch(hash1.to_string(), hash2.to_string());
        }
    }

    ComparisonResult::Mismatch(hash1.to_string(), hash2.to_string())
}

fn is_partial_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split("***").collect();

    if parts.is_empty() {
        return false;
    }

    let mut pos = 0;

    if !parts[0].is_empty() {
        if !text.starts_with(parts[0]) {
            return false;
        }
        pos = parts[0].len();
    }

    for i in 1..parts.len() - 1 {
        if let Some(found_pos) = text[pos..].find(parts[i]) {
            pos += found_pos + parts[i].len();
        } else {
            return false;
        }
    }

    if !parts[parts.len() - 1].is_empty() {
        if !text[pos..].ends_with(parts[parts.len() - 1]) {
            return false;
        }
    }

    true
}

fn find_only_in_file2(
    hashes1: &Vec<HashEntry>,
    hashes2: &Vec<HashEntry>,
) -> Vec<(String, ComparisonResult)> {
    let mut results = Vec::new();

    for entry2 in hashes2 {
        let found = hashes1.iter().any(|e| e.filepath == entry2.filepath);
        if !found {
            results.push((
                entry2.filepath.clone(),
                ComparisonResult::OnlyInFile2(entry2.hash.clone()),
            ));
        }
    }

    results
}

fn display_results(results: &[(String, ComparisonResult)]) {
    println!("=== Hash Comparison Results ===\n");

    let mut matches = 0;
    let mut partial_matches = 0;
    let mut mismatches = 0;
    let mut only_in_file1 = 0;
    let mut only_in_file2 = 0;

    let mut sorted_results = results.to_vec();
    sorted_results.sort_by(|a, b| a.0.cmp(&b.0));

    for (filepath, result) in &sorted_results {
        match result {
            ComparisonResult::FullMatch(_) => {
                println!("[FULL MATCH] {}", filepath);
                matches += 1;
            }
            ComparisonResult::PartialMatch(hash1, hash2) => {
                println!("[PARTIAL MATCH] {}", filepath);
                println!("  File1: {}", hash1);
                println!("  File2: {}", hash2);
                partial_matches += 1;
            }
            ComparisonResult::Mismatch(hash1, hash2) => {
                println!("[MISMATCH] {}", filepath);
                println!("  File1: {}", hash1);
                println!("  File2: {}", hash2);
                mismatches += 1;
            }
            ComparisonResult::OnlyInFile1(hash) => {
                println!("[ONLY IN FILE1] {} (hash: {})", filepath, hash);
                only_in_file1 += 1;
            }
            ComparisonResult::OnlyInFile2(hash) => {
                println!("[ONLY IN FILE2] {} (hash: {})", filepath, hash);
                only_in_file2 += 1;
            }
        }
    }

    println!("\n=== Summary ===");
    println!("Full Matches:    {}", matches);
    println!("Partial Matches: {}", partial_matches);
    println!("Mismatches:      {}", mismatches);
    println!("Only in File1:   {}", only_in_file1);
    println!("Only in File2:   {}", only_in_file2);
    println!(
        "Total entries:   {}",
        matches + partial_matches + mismatches + only_in_file1 + only_in_file2
    );
}

fn export_to_csv(
    results: &[(String, ComparisonResult)],
    output_dir: &str,
    file1: &str,
    file2: &str,
) -> std::io::Result<()> {
    std::fs::create_dir_all(output_dir)?;

    let file1_name = Path::new(file1)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(file1);
    let file2_name = Path::new(file2)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(file2);

    let mut full_matches = Vec::new();
    let mut partial_matches = Vec::new();
    let mut mismatches = Vec::new();
    let mut only_in_file1 = Vec::new();
    let mut only_in_file2 = Vec::new();

    let mut stats = ComparisonStats {
        full_matches: 0,
        partial_matches: 0,
        mismatches: 0,
        only_in_file1: 0,
        only_in_file2: 0,
    };

    for (filepath, result) in results {
        match result {
            ComparisonResult::FullMatch(hash) => {
                full_matches.push((filepath.clone(), hash.clone()));
                stats.full_matches += 1;
            }
            ComparisonResult::PartialMatch(hash1, hash2) => {
                partial_matches.push((filepath.clone(), hash1.clone(), hash2.clone()));
                stats.partial_matches += 1;
            }
            ComparisonResult::Mismatch(hash1, hash2) => {
                mismatches.push((filepath.clone(), hash1.clone(), hash2.clone()));
                stats.mismatches += 1;
            }
            ComparisonResult::OnlyInFile1(hash) => {
                only_in_file1.push((filepath.clone(), hash.clone()));
                stats.only_in_file1 += 1;
            }
            ComparisonResult::OnlyInFile2(hash) => {
                only_in_file2.push((filepath.clone(), hash.clone()));
                stats.only_in_file2 += 1;
            }
        }
    }

    let full_matches_path = format!("{}/Result_Full_Matches.csv", output_dir);
    let mut file = File::create(&full_matches_path)?;
    writeln!(file, "\"Path\",\"hash\"")?;
    for (path, hash) in &full_matches {
        writeln!(file, "\"{}\",\"{}\"", escape_csv(path), escape_csv(hash))?;
    }

    let partial_matches_path = format!("{}/Result_Partial_Match.csv", output_dir);
    let mut file = File::create(&partial_matches_path)?;
    writeln!(
        file,
        "\"Path\",\"hash in {}\",\"hash in {}\"",
        escape_csv(file1_name),
        escape_csv(file2_name)
    )?;
    for (filepath, hash1, hash2) in &partial_matches {
        writeln!(
            file,
            "\"{}\",\"{}\",\"{}\"",
            escape_csv(filepath),
            escape_csv(hash1),
            escape_csv(hash2)
        )?;
    }

    let mismatches_path = format!("{}/Result_Mismatches.csv", output_dir);
    let mut file = File::create(&mismatches_path)?;
    writeln!(
        file,
        "\"Path\",\"hash in {}\",\"hash in {}\"",
        escape_csv(file1_name),
        escape_csv(file2_name)
    )?;
    for (filepath, hash1, hash2) in &mismatches {
        writeln!(
            file,
            "\"{}\",{},{}",
            escape_csv(filepath),
            hash1,
            hash2
        )?;
    }

    let only_file1_path = format!("{}/Result_Only_in_{}.csv", output_dir, escape_csv(file1_name));
    let mut file = File::create(&only_file1_path)?;
    writeln!(file, "\"Path\",\"hash\"")?;
    for (filepath, hash) in &only_in_file1 {
        writeln!(file, "\"{}\",\"{}\"", escape_csv(filepath), escape_csv(hash))?;
    }

    let only_file2_path = format!("{}/Result_Only_in_{}.csv", output_dir, escape_csv(file2_name));
    let mut file = File::create(&only_file2_path)?;
    writeln!(file, "\"Path\",\"hash\"")?;
    for (filepath, hash) in &only_in_file2 {
        writeln!(file, "\"{}\",\"{}\"", escape_csv(filepath), escape_csv(hash))?;
    }

    let summary_path = format!("{}/Result_summary.csv", output_dir);
    let mut file = File::create(&summary_path)?;
    writeln!(file, "\"Full Matches\",{}", stats.full_matches)?;
    writeln!(file, "\"Partial Matches\",{}", stats.partial_matches)?;
    writeln!(file, "\"Mismatches\",{}", stats.mismatches)?;
    writeln!(
        file,
        "\"Only in {}\",{}",
        escape_csv(file1_name),
        stats.only_in_file1
    )?;
    writeln!(
        file,
        "\"Only in {}\",{}",
        escape_csv(file2_name),
        stats.only_in_file2
    )?;
    writeln!(
        file,
        "\"Total entries\",{}",
        stats.full_matches
            + stats.partial_matches
            + stats.mismatches
            + stats.only_in_file1
            + stats.only_in_file2
    )?;

    Ok(())
}

fn escape_csv(value: &str) -> String {
    if value.contains('"') || value.contains(',') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
