#!/bin/bash
set -e

echo "Building mdbook..."
cd docs
mdbook build

echo "Setting up gh-pages worktree..."
# Clean up any existing gh-pages state
rm -rf gh-pages
git worktree prune

# Delete local gh-pages branch if it exists
git branch -D gh-pages 2>/dev/null || true

# Create orphan gh-pages branch (clean history)
git worktree add --orphan -B gh-pages gh-pages

echo "Copying built book..."
cp -rT book/ gh-pages/

cd gh-pages

git add -A
git commit -m "Deploy documentation $(date +'%Y-%m-%d %H:%M:%S')"

echo "Pushing to gh-pages..."
git push origin +gh-pages

cd ..
git worktree remove gh-pages

echo "✓ Documentation deployed to gh-pages branch!"
