#!/bin/bash

# Check if Homebrew is installed
if ! command -v brew &> /dev/null
then
    echo "Homebrew not found. Please install it first: https://brew.sh/"
    exit 1
fi

# Install dependencies for egui/eframe and avif support
brew install \
    cmake \
    nasm \
    pkg-config

echo "System dependencies installed."
