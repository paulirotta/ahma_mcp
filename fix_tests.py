#!/usr/bin/env python3
"""
Fix SubcommandConfig structs in test files to include new fields.
"""
import re
import sys
from pathlib import Path

def fix_file(filepath):
    content = filepath.read_text()
    
    # Pattern to match SubcommandConfig initialization that's missing sequence fields
    # We look for lines with "subcommand: None," followed by availability_check or install_instructions
    pattern = r'(\s+)subcommand: None,\n(\s+)(availability_check|install_instructions)'
    replacement = r'\1subcommand: None,\n\1sequence: None,\n\1step_delay_ms: None,\n\2\3'
    
    new_content = re.sub(pattern, replacement, content)
    
    if new_content != content:
        filepath.write_text(new_content)
        print(f"Fixed {filepath}")
        return True
    return False

def main():
    test_dir = Path("tests")
    fixed_count = 0
    
    for test_file in test_dir.glob("*.rs"):
        if fix_file(test_file):
            fixed_count += 1
    
    print(f"\nFixed {fixed_count} files")

if __name__ == "__main__":
    main()
