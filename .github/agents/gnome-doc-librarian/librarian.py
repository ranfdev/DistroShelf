#!/usr/bin/env python3
"""
GNOME Documentation Librarian
Command-line utility for exploring GNOME documentation in /usr/share/runtime/docs/doc/.

This script provides CLI-focused helpers to list projects, inspect files and
render HTML/Markdown into terminal-friendly text (commands: list, projects,
project, search, info, open, show, cat, grep). It's intended for environments
without a graphical browser so documentation can be read comfortably from the
terminal.
"""

import os
import sys
import re
import argparse
import subprocess
import json
from pathlib import Path
from typing import List, Dict, Optional, Tuple
import html
from html.parser import HTMLParser
import textwrap


class FlatpakRunner:
    """Utility class to handle execution within Flatpak when needed."""
    
    FLATPAK_CMD = "org.gnome.Sdk//49"
    SCRIPT_PATH = Path(__file__).resolve()
    
    @staticmethod
    def is_inside_flatpak() -> bool:
        """Check if we're currently running inside a Flatpak."""
        return os.path.exists("/.flatpak-info") or os.environ.get("FLATPAK_ID") is not None
    
    @staticmethod
    def is_doc_available() -> bool:
        """Check if documentation is accessible."""
        return Path("/usr/share/runtime/docs/doc/").exists()
    
    @staticmethod
    def run_in_flatpak(args: List[str]) -> Tuple[int, str, str]:
        """Run the script inside the Flatpak shell with mounted script file.
        
        Returns: (returncode, stdout, stderr)
        """
        flatpak_cmd = [
            "flatpak", "run", "--devel",
            f"--filesystem={FlatpakRunner.SCRIPT_PATH}",
            "--command=python3",
            FlatpakRunner.FLATPAK_CMD,
            str(FlatpakRunner.SCRIPT_PATH)
        ] + args
        
        try:
            result = subprocess.run(
                flatpak_cmd,
                capture_output=True,
                text=True,
                timeout=30
            )
            return result.returncode, result.stdout, result.stderr
        except subprocess.TimeoutExpired:
            return -1, "", "Command timed out"
        except Exception as e:
            return -1, "", str(e)


class DocBrowser:
    """Main class for browsing GNOME documentation."""
    
    DOC_ROOT = Path("/usr/share/runtime/docs/doc/")
    
    def __init__(self):
        """Initialize the documentation browser."""
        # Check if docs are available locally
        if not self.DOC_ROOT.exists():
            # Check if we can access via Flatpak
            if not FlatpakRunner.is_inside_flatpak():
                # Re-run this script inside Flatpak
                returncode, stdout, stderr = FlatpakRunner.run_in_flatpak(sys.argv[1:])
                # Print the output and exit with the same return code
                if stdout:
                    print(stdout, end='')
                if stderr:
                    print(stderr, end='', file=sys.stderr)
                sys.exit(returncode)
            else:
                print(f"Error: Documentation directory {self.DOC_ROOT} not found")
                sys.exit(1)
        
        # Keep resolved paths to make containment checks robust
        self.current_path = self.DOC_ROOT.resolve()
        self.history = [self.DOC_ROOT]
        self.history_index = 0
    
    def list_root_projects(self) -> List[str]:
        """List all available documentation projects (top-level directories)."""
        try:
            items = sorted([d.name for d in self.DOC_ROOT.iterdir() if d.is_dir()])
            return items
        except PermissionError:
            print("Error: Permission denied accessing documentation directory")
            return []
    
    def list_contents(self, path: Optional[Path] = None) -> Dict[str, List[str]]:
        """List contents of a directory, separated into directories and files."""
        target = path or self.current_path
        
        try:
            dirs = []
            files = []
            
            for item in sorted(target.iterdir()):
                if item.is_dir():
                    dirs.append(item.name)
                else:
                    files.append(item.name)
            
            return {"dirs": dirs, "files": files}
        except PermissionError:
            print(f"Error: Permission denied accessing {target}")
            return {"dirs": [], "files": []}
    
    def navigate(self, relative_path: str) -> bool:
        """Navigate to a directory. Supports .. for parent."""
        if relative_path == "..":
            if self.current_path != self.DOC_ROOT.resolve():
                new_path = (self.current_path.parent).resolve()
                if self._is_within_doc_root(new_path):
                    self.current_path = new_path
                    self._update_history()
                    return True
            return False
        
        target = (self.current_path / relative_path).resolve()
        if target.exists() and target.is_dir() and self._is_within_doc_root(target):
            self.current_path = target
            self._update_history()
            return True
        
        return False
    
    def _update_history(self):
        """Update navigation history."""
        self.history = self.history[:self.history_index + 1]
        # store resolved path so subsequent checks are consistent
        self.history.append(self.current_path)
        self.history_index = len(self.history) - 1

    def _is_within_doc_root(self, path: Path) -> bool:
        """Return True if `path` is equal to or contained within DOC_ROOT.

        Avoids using Path ordering operators which can raise TypeError on
        some Python versions. Uses resolved paths and parent checks.
        """
        try:
            root = self.DOC_ROOT.resolve()
            p = path.resolve()
            return p == root or root in p.parents
        except Exception:
            return False
    
    def back(self) -> bool:
        """Navigate back in history."""
        if self.history_index > 0:
            self.history_index -= 1
            self.current_path = self.history[self.history_index]
            return True
        return False
    
    def forward(self) -> bool:
        """Navigate forward in history."""
        if self.history_index < len(self.history) - 1:
            self.history_index += 1
            self.current_path = self.history[self.history_index]
            return True
        return False
    
    def search_files(self, pattern: str, max_results: int = 50) -> List[Path]:
        """Search for files matching a pattern (regex or simple name search)."""
        try:
            regex = re.compile(pattern, re.IGNORECASE)
        except re.error:
            # Fallback to simple substring search
            regex = re.compile(re.escape(pattern), re.IGNORECASE)
        
        results = []
        
        try:
            for file_path in self.DOC_ROOT.rglob("*"):
                if file_path.is_file() and regex.search(file_path.name):
                    results.append(file_path)
                    if len(results) >= max_results:
                        break
        except PermissionError:
            pass
        
        return sorted(results)
    
    def extract_html_title(self, file_path: Path) -> Optional[str]:
        """Extract title from HTML file."""
        try:
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                content = f.read(2000)
            
            # Try to find title tag
            match = re.search(r'<title[^>]*>([^<]+)</title>', content, re.IGNORECASE)
            if match:
                title = html.unescape(match.group(1)).strip()
                return title
            
            # Try to find h1 heading
            match = re.search(r'<h1[^>]*>([^<]+)</h1>', content, re.IGNORECASE)
            if match:
                title = html.unescape(match.group(1)).strip()
                return title
        except Exception:
            pass
        
        return None

    # --- rendering utilities -------------------------------------------------
    class _HTMLTextExtractor(HTMLParser):
        """Simple HTML -> plain-text extractor suitable for terminal viewing.

        It preserves block-level breaks for headings, paragraphs, lists and
        keeps preformatted blocks intact. Links are shown as text (url) where
        possible.
        """

        def __init__(self):
            super().__init__()
            self._parts: List[str] = []
            self._in_pre = False
            self._last_was_block = True

        def handle_starttag(self, tag, attrs):
            if tag in ("p", "div", "section", "header", "article"):
                if not self._last_was_block:
                    self._parts.append("\n")
                self._last_was_block = True
            elif tag in ("br",):
                self._parts.append("\n")
            elif tag in ("h1", "h2", "h3", "h4", "h5", "h6", "li"):
                if not self._last_was_block:
                    self._parts.append("\n")
                self._last_was_block = True
            elif tag == 'pre':
                self._in_pre = True

        def handle_endtag(self, tag):
            if tag == 'pre':
                self._in_pre = False
            if tag in ("p", "div", "section", "article", "header", "h1", "h2", "h3", "h4", "h5", "h6"):
                self._parts.append("\n")
                self._last_was_block = True

        def handle_data(self, data):
            if not data:
                return
            if self._in_pre:
                # keep exact content inside <pre>
                self._parts.append(data)
                self._last_was_block = False
                return

            text = ' '.join(data.split())
            if not text:
                return

            # Ensure words don't get concatenated when later joining; if the
            # previous content was a non-block text token, insert a leading
            # space when appending the next token.
            if not self._parts or self._last_was_block:
                self._parts.append(text)
            else:
                self._parts.append(' ' + text)
            self._last_was_block = False

        def get_text(self) -> str:
            # join without adding spaces to preserve explicitly inserted
            # newlines and preformatted content
            raw = ''.join(self._parts)
            # Reduce runs of multiple blank lines to a maximum of two
            cleaned = re.sub(r"\n\s*\n+", "\n\n", html.unescape(raw))
            return cleaned.strip()

    def render_file_as_text(self, file_path: Path, raw: bool = False) -> str:
        """Return a terminal-friendly string for the given file.

        - For HTML: uses a simple HTML parser to extract visible text.
        - For markdown (.md): returns the file content (plain), leaving
          a thin conversion to the terminal (no fancy markdown rendering).
        - For everything else: returns file text as-is, or a message if
          binary/unreadable.
        """
        try:
            suffix = file_path.suffix.lower()
            text = file_path.read_text(encoding='utf-8', errors='ignore')

            if raw or suffix not in ('.html', '.htm', '.md'):
                return text

            if suffix in ('.md',):
                # Minimal markdown normalization: collapse multiple blank lines
                return '\n'.join([line.rstrip() for line in text.splitlines()])

            # HTML: parse into a readable text representation
            extractor = DocBrowser._HTMLTextExtractor()
            extractor.feed(text)
            extracted = extractor.get_text()

            # For nice terminal output: wrap normal paragraphs but leave
            # preformatted blocks intact. We split into paragraphs and wrap
            # only paragraphs which don't contain explicit newlines.
            parts = [p for p in extracted.split('\n\n')]
            out_parts = []
            for p in parts:
                if '\n' in p:
                    # likely preformatted or a block with internal newlines, keep as-is
                    out_parts.append(p)
                else:
                    out_parts.append(textwrap.fill(p, width=100))

            return '\n\n'.join(out_parts)

        except Exception as e:
            return f"[Error reading {file_path}: {e}]"
    
    def get_file_info(self, file_path: Path) -> Dict[str, str]:
        """Get information about a file."""
        try:
            size = file_path.stat().st_size
            relative_path = file_path.relative_to(self.DOC_ROOT)
            
            info = {
                "path": str(relative_path),
                "size": self._format_size(size),
                "type": file_path.suffix.lower() or "file"
            }
            
            if file_path.suffix.lower() == ".html":
                title = self.extract_html_title(file_path)
                if title:
                    info["title"] = title
            
            return info
        except Exception as e:
            return {"error": str(e)}
    
    def _format_size(self, size: int) -> str:
        """Format file size in human-readable format."""
        for unit in ['B', 'KB', 'MB', 'GB']:
            if size < 1024.0:
                return f"{size:.1f} {unit}"
            size /= 1024.0
        return f"{size:.1f} TB"
    
    def get_project_summary(self, project_name: str) -> Dict:
        """Get summary information about a documentation project."""
        project_path = self.DOC_ROOT / project_name
        
        if not project_path.exists():
            return {"error": f"Project '{project_name}' not found"}
        
        try:
            html_files = len(list(project_path.rglob("*.html")))
            md_files = len(list(project_path.rglob("*.md")))
            total_size = sum(f.stat().st_size for f in project_path.rglob("*") if f.is_file())
            
            # Try to find index or main file
            index_file = None
            for name in ['index.html', 'README.html', 'index.md']:
                candidate = project_path / name
                if candidate.exists():
                    index_file = name
                    break
            
            other_files = 0  # Simplified for now
            
            return {
                "project": project_name,
                "html_files": html_files,
                "markdown_files": md_files,
                "other_files": other_files,
                "total_size": self._format_size(total_size),
                "index_file": index_file
            }
        except Exception as e:
            return {"error": str(e)}
    
    def print_current_dir(self):
        """Print current directory contents nicely."""
        contents = self.list_contents()
        
        print(f"\n📁 {self.current_path.relative_to(self.DOC_ROOT) or 'Root'}")
        print("=" * 60)
        
        if not contents["dirs"] and not contents["files"]:
            print("(empty)")
            return
        
        if contents["dirs"]:
            print("\n📂 Directories:")
            for d in contents["dirs"]:
                print(f"  > {d}/")
        
        if contents["files"]:
            print(f"\n📄 Files ({len(contents['files'])}):")
            for f in contents["files"][:20]:  # Show first 20
                print(f"  • {f}")
            if len(contents["files"]) > 20:
                print(f"  ... and {len(contents['files']) - 20} more files")


def cmd_list(args, browser: DocBrowser):
    """List current directory contents."""
    browser.print_current_dir()


def cmd_projects(args, browser: DocBrowser):
    """List all available documentation projects."""
    projects = browser.list_root_projects()
    print(f"\n📚 Available Documentation Projects ({len(projects)}):")
    for project in projects:
        print(f"  • {project}")


def cmd_project_info(args, browser: DocBrowser):
    """Get information about a specific project."""
    info = browser.get_project_summary(args.project)
    
    if "error" in info:
        print(f"Error: {info['error']}")
        return
    
    print(f"\n📊 Project: {info.get('project', args.project)}")
    for key, value in info.items():
        if key != "project":
            print(f"  {key.replace('_', ' ').title()}: {value}")


def cmd_search(args, browser: DocBrowser):
    """Search for files matching a pattern."""
    results = browser.search_files(args.pattern, max_results=args.limit)
    print(f"\n🔍 Search results for '{args.pattern}' ({len(results)} found):")
    
    for result in results:
        rel = result.relative_to(browser.DOC_ROOT)
        print(f"  • {rel}")
    
    if len(results) >= args.limit:
        print(f"  ... (limited to {args.limit} results)")


def cmd_file_info(args, browser: DocBrowser):
    """Get information about a file."""
    file_path = browser.current_path / args.file
    if not file_path.exists():
        file_path = browser.DOC_ROOT / args.file
    
    if not file_path.exists():
        print(f"Error: File '{args.file}' not found")
        return
    
    info = browser.get_file_info(file_path)
    
    if "error" in info:
        print(f"Error: {info['error']}")
        return
    
    print(f"\n📋 File Info: {args.file}")
    for key, value in info.items():
        print(f"  {key.title()}: {value}")


# 'open' command removed — environments for this tool are terminal-only
# Use 'show' to render HTML/MD or 'cat' for raw output instead.


def _resolve_file_arg(browser: DocBrowser, file_arg: str) -> Optional[Path]:
    """Resolve a file path given by the user, first trying current_path then the doc root."""
    p = browser.current_path / file_arg
    if p.exists():
        return p
    p = browser.DOC_ROOT / file_arg
    if p.exists():
        return p
    return None


def cmd_show(args, browser: DocBrowser):
    """Print a file to stdout, rendering HTML/MD unless --raw is provided."""
    file_path = _resolve_file_arg(browser, args.file)
    if not file_path:
        print(f"Error: File '{args.file}' not found")
        return

    if not file_path.is_file():
        print(f"Error: '{args.file}' is not a file")
        return

    raw = getattr(args, 'raw', False)
    content = browser.render_file_as_text(file_path, raw=raw)

    # Apply a lines limit if requested
    n = getattr(args, 'lines', None)
    if n is not None:
        lines = content.splitlines()
        to_print = '\n'.join(lines[:n])
    else:
        to_print = content

    print(to_print)


def cmd_cat(args, browser: DocBrowser):
    """Alias to show - outputs file raw (like unix cat)."""
    # Build a fake args object to reuse cmd_show
    class A: pass
    a = A()
    a.file = args.file
    a.raw = True
    a.lines = None
    cmd_show(a, browser)


def cmd_grep(args, browser: DocBrowser):
    """Search inside files for a given regex/string and print matches with context."""
    try:
        flags = re.IGNORECASE if args.ignore_case else 0
        pattern = re.compile(args.pattern, flags)
    except re.error:
        print("Invalid pattern")
        return

    max_results = args.limit
    context = args.context
    results_found = 0

    for file_path in browser.DOC_ROOT.rglob("*"):
        if not file_path.is_file():
            continue
        try:
            text = file_path.read_text(encoding='utf-8', errors='ignore')
        except Exception:
            continue

        lines = text.splitlines()
        for i, line in enumerate(lines):
            if pattern.search(line):
                print(f"{file_path.relative_to(browser.DOC_ROOT)}:{i+1}:{line.rstrip()}")
                results_found += 1
                if results_found >= max_results:
                    print(f"...Reached result limit of {max_results}")
                    return

                # print context lines around this match if requested (both
                # before and after), but don't duplicate the matching line.
                if context > 0:
                    start = max(0, i - context)
                    end = min(len(lines), i + context + 1)
                    for j in range(start, end):
                        if j == i:
                            continue
                        print(f"  {j+1}: {lines[j].rstrip()}")

    if results_found == 0:
        print("No matches found")


def create_parser() -> argparse.ArgumentParser:
    """Create and configure the argument parser."""
    parser = argparse.ArgumentParser(
        prog="librarian",
        description="Browse and explore GNOME documentation from /usr/share/runtime/docs/doc/",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s list                    List contents of root documentation
  %(prog)s projects                List all available projects
  %(prog)s project gtk4            Get information about GTK4 documentation
  %(prog)s search "Button"         Search for files containing "Button"
    %(prog)s info gtk4/class.Button.html  Get info about a specific file
    %(prog)s show gtk4/index.html    Render HTML/MD to terminal-friendly text
    %(prog)s cat gtk4/index.html     Print raw file contents
    %(prog)s grep "signal" -i       Search inside docs for 'signal' (case insensitive)
        """
    )
    
    subparsers = parser.add_subparsers(dest="command", help="Available commands")
    
    # list command
    subparsers.add_parser(
        "list",
        help="List contents of the root documentation directory"
    )
    
    # projects command
    subparsers.add_parser(
        "projects",
        help="List all available documentation projects"
    )
    
    # project info command
    project_parser = subparsers.add_parser(
        "project",
        help="Get information about a specific documentation project"
    )
    project_parser.add_argument(
        "project",
        help="Name of the project (e.g., gtk4, libadwaita, glib)"
    )
    
    # search command
    search_parser = subparsers.add_parser(
        "search",
        help="Search for files matching a pattern"
    )
    search_parser.add_argument(
        "pattern",
        help="Search pattern (regex or substring)"
    )
    search_parser.add_argument(
        "-l", "--limit",
        type=int,
        default=50,
        help="Maximum number of results to return (default: 50)"
    )
    
    # info command
    info_parser = subparsers.add_parser(
        "info",
        help="Get detailed information about a file"
    )
    info_parser.add_argument(
        "file",
        help="Path to file (relative to doc root or current directory)"
    )
    
    # (no "open" command — environment is terminal-only; use show/cat)

    # show command - render content to terminal
    show_parser = subparsers.add_parser(
        "show",
        help="Render a documentation file to terminal-friendly text (HTML/MD rendered)."
    )
    show_parser.add_argument("file", help="Path to file (relative to doc root or current directory)")
    show_parser.add_argument("-r", "--raw", action="store_true", help="Print raw file contents without rendering")
    show_parser.add_argument("-n", "--lines", type=int, help="Only print the first N lines")

    # cat command - alias for raw printing
    cat_parser = subparsers.add_parser(
        "cat",
        help="Print a file's raw contents (alias for show --raw)."
    )
    cat_parser.add_argument("file", help="Path to file (relative to doc root or current directory)")

    # grep command - search inside documentation files
    grep_parser = subparsers.add_parser(
        "grep",
        help="Search inside documentation files for a pattern"
    )
    grep_parser.add_argument("pattern", help="Regex or substring to search for")
    grep_parser.add_argument("-i", "--ignore-case", action="store_true", help="Case insensitive matching")
    grep_parser.add_argument("-l", "--limit", type=int, default=50, help="Maximum number of matches to show")
    grep_parser.add_argument("-C", "--context", type=int, default=0, help="Show N context lines after match")
    
    return parser


def main():
    """Main entry point."""
    parser = create_parser()
    args = parser.parse_args()
    
    browser = DocBrowser()
    
    # Map commands to their handlers
    commands = {
        "list": cmd_list,
        "projects": cmd_projects,
        "project": cmd_project_info,
        "search": cmd_search,
        "info": cmd_file_info,
        "show": cmd_show,
        "cat": cmd_cat,
        "grep": cmd_grep,
    }
    
    # Execute the command
    if args.command in commands:
        try:
            commands[args.command](args, browser)
        except Exception as e:
            print(f"Error: {e}", file=sys.stderr)
            sys.exit(1)
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == "__main__":
    main()
