# Arch Linux Update Checker - Development Plan

## Project Overview

A system tray application that monitors for Arch Linux package updates using yay and displays the count in the system tray with notifications.

## Tech Stack

**Language**: Python 3.11+
**GUI Framework**: PyQt6
**Package Manager Interface**: yay (via subprocess)
**Packaging**: Python package with entry point

## Core Features (MVP)

1. Update Checking

    - Run yay -Qu to check for available updates
    - Parse output to count updates
    - Run checks on interval (configurable, default: 1 hour)
    - Manual refresh option via tray menu

2. System Tray Icon

    **Display different icons based on state**:

    - No updates available (green/neutral)
    - Updates available (orange/yellow with count badge)
    - Checking for updates (spinner/loading)
    - Error state (red)


    Tooltip showing update count or last check time

3. Context Menu

    - "**Check Now**" - Force immediate update check
    - "**Show Updates**" - Display list of available updates in dialog
    - "**Update System**" - Launch terminal with yay -Syu
    - "**Settings**" - Open configuration dialog
    - "**Quit**" - Exit application

4. Notifications

    - Desktop notification when new updates are detected
    - Configurable notification behavior (always/once/never)

5. Configuration

    - Check interval (minutes/hours)
    - Notification preferences
    - Auto-start on login (via .desktop file)
    - Terminal emulator preference for launching updates