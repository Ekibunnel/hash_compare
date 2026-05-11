# hash_compare

A Rust program to compare hashes and filepaths stored in two text files, with support for partial hash matching.

Can output result as multiple .csv if a folder is provided as the 3rd argument.

## Hash Format

Each line in the input files must follow this format:
```
<hash> *<filepath>
```

Example:
```
0123456789abcdfe *path/of/the/file1.test
```

### Partial Hash Matching with Wildcards

For partial matching, use `***` (this is not regex, you must use exactly those) to represent unknown characters:

```
1333333333333337 *leet.test      (File 1 - full hash)
13***37 *leet.test               (File 2 - partial hash)
```

These will be matched as a **PARTIAL MATCH** because the hash starts with `13`, has unknown middle characters, and ends with `37`.

## Building

### Requirements
- Rust 1.56 or later (https://rustup.rs/)

### Compile
```bash
cargo build --release
```

## Running

```bash
./target/release/hash_compare <file1> <file2> [output_dir]
```

Example:
```bash
./hash_compare fileA.txt fileB.txt ./results_File
./hash_compare fileA.txt fileB.txt
```


## Example Output
```
Reading files...
File1: 6 entries, File2: 7 entries

Progress: [==================================================] 100.0% (6/6)
Results exported to: ./results_File
```
```
Reading files...
File1: 6 entries, File2: 7 entries

=== Hash Comparison Results ===

[PARTIAL MATCH] leet.test
  File1: 1333333333333337
  File2: 13***37
[FULL MATCH] path/of/the/file1.test
[FULL MATCH] path/of/the/file2.test
[ONLY IN FILE1] path/of/the/file3.test (hash: abcdef1234567890)
[ONLY IN FILE2] path/of/the/file3_but_it_was_renamed_or_moved.test (hash: abcdef1234567890)
[MISMATCH] path/of/the/file4.test
  File1: 1111111111111111
  File2: 2222222222222222
[ONLY IN FILE2] path/of/the/file5.test (hash: 9999999999999999)
[PARTIAL MATCH] testpartialmatch
  File1: abcdef***3456700
  File2: abcdefffffffffff12456786873456700

=== Summary ===
Full Matches:    2
Partial Matches: 2
Mismatches:      1
Only in File1:   1
Only in File2:   2
Total entries:   8
```
