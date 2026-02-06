#!/usr/bin/env python3
"""
Updates the bundled Distrobox release version and SHA256 hash.

This script:
1. Fetches the latest release from GitHub
2. Downloads the release tarball
3. Computes its SHA256 hash
4. Updates the constants in src/distrobox_downloader.rs
"""

import hashlib
import json
import re
import sys
import tempfile
import urllib.request
from pathlib import Path
from typing import Optional, Tuple


GITHUB_API_URL = "https://api.github.com/repos/89luca89/distrobox/releases/latest"
DISTROBOX_DOWNLOADER_PATH = Path(__file__).parent.parent / "src" / "distrobox_downloader.rs"


def fetch_latest_release() -> Tuple[str, str]:
    """
    Fetches the latest Distrobox release information from GitHub.
    
    Returns:
        Tuple of (version, download_url)
    """
    print("Fetching latest Distrobox release from GitHub...", file=sys.stderr)
    
    req = urllib.request.Request(
        GITHUB_API_URL,
        headers={"Accept": "application/vnd.github+json"}
    )
    
    with urllib.request.urlopen(req) as response:
        data = json.loads(response.read().decode('utf-8'))
    
    tag_name = data['tag_name']
    # Remove 'v' prefix if present
    version = tag_name.lstrip('v')
    
    # Construct download URL
    download_url = f"https://github.com/89luca89/distrobox/archive/refs/tags/{tag_name}.tar.gz"
    
    print(f"Latest release: {version}", file=sys.stderr)
    print(f"Download URL: {download_url}", file=sys.stderr)
    
    return version, download_url


def compute_sha256(url: str) -> str:
    """
    Downloads a file and computes its SHA256 hash.
    
    Args:
        url: URL of the file to download
        
    Returns:
        The SHA256 hash as a hexadecimal string
    """
    print(f"Downloading {url} to compute hash...", file=sys.stderr)
    
    sha256_hash = hashlib.sha256()
    
    with urllib.request.urlopen(url) as response:
        # Read in chunks to handle large files
        while chunk := response.read(8192):
            sha256_hash.update(chunk)
    
    hash_hex = sha256_hash.hexdigest()
    print(f"SHA256: {hash_hex}", file=sys.stderr)
    
    return hash_hex


def update_rust_constants(version: str, sha256: str) -> None:
    """
    Updates the version and SHA256 constants in the Rust source file.
    
    Args:
        version: The new version string
        sha256: The new SHA256 hash
    """
    if not DISTROBOX_DOWNLOADER_PATH.exists():
        raise FileNotFoundError(f"Could not find {DISTROBOX_DOWNLOADER_PATH}")
    
    print(f"Updating {DISTROBOX_DOWNLOADER_PATH}...", file=sys.stderr)
    
    content = DISTROBOX_DOWNLOADER_PATH.read_text()
    
    # Update version
    content = re.sub(
        r'pub const DISTROBOX_VERSION: &str = "[^"]+";',
        f'pub const DISTROBOX_VERSION: &str = "{version}";',
        content
    )
    
    # Update SHA256
    content = re.sub(
        r'pub const DISTROBOX_SHA256: &str =\s*"[^"]+";',
        f'pub const DISTROBOX_SHA256: &str =\n    "{sha256}";',
        content
    )
    
    DISTROBOX_DOWNLOADER_PATH.write_text(content)
    
    print("✓ Updated successfully", file=sys.stderr)


def get_current_version() -> Optional[str]:
    """
    Reads the current version from the Rust source file.
    
    Returns:
        The current version string or None if not found
    """
    if not DISTROBOX_DOWNLOADER_PATH.exists():
        return None
    
    content = DISTROBOX_DOWNLOADER_PATH.read_text()
    match = re.search(r'pub const DISTROBOX_VERSION: &str = "([^"]+)";', content)
    
    return match.group(1) if match else None


def main() -> int:
    """Main entry point."""
    try:
        current_version = get_current_version()
        if current_version:
            print(f"Current bundled version: {current_version}", file=sys.stderr)
        
        version, download_url = fetch_latest_release()
        
        if current_version == version:
            print(f"Already up to date (version {version})", file=sys.stderr)
            return 0
        
        sha256 = compute_sha256(download_url)
        update_rust_constants(version, sha256)
        
        print(f"\n✓ Updated from {current_version} to {version}", file=sys.stderr)
        print(f"  Version: {version}")
        print(f"  SHA256: {sha256}")
        
        return 0
        
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
