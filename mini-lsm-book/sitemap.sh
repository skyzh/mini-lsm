#!/bin/bash
# mini-lsm-book © 2022-2025 by Alex Chi Z is licensed under CC BY-NC-SA 4.0


mdbook build
sscli -b https://skyzh.github.io/mini-lsm -r book -f xml -o > src/sitemap.xml
sscli -b https://skyzh.github.io/mini-lsm -r book -f txt -o > src/sitemap.txt
