#!/bin/bash

# Update package list
sudo apt-get update

# Install dependencies for egui/eframe and Wayland support
sudo apt-get install -y \
    libwayland-dev \
    libx11-dev \
    libxkbcommon-dev \
    libegl1-mesa-dev \
    libdbus-1-dev \
    libgtk-3-dev \
    build-essential \
    pkg-config \
    libssl-dev \
    libclang-dev \
    cmake \
    nasm

echo "System dependencies installed."
