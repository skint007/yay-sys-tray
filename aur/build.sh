#!/bin/bash

makepkg -si --noconfirm
systemctl --user restart yay-sys-tray.service