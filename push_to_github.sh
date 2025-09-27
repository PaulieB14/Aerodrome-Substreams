#!/bin/bash

# Navigate to the project directory
cd ~/Desktop/aerodrome-substreams

# Add all files
git add .

# Commit with message
git commit -m "Initial commit: Aerodrome Substreams for Base blockchain v0.1.1"

# Add remote repository
git remote add origin git@github.com:PaulieB14/Aerodrome-Substreams.git

# Push to GitHub
git push -u origin main

echo "Done! Repository pushed to GitHub"
