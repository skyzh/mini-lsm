#!/bin/bash

mdbook build
sscli -b https://skyzh.github.io/mini-lsm -r book -f xml -o > src/sitemap.xml
sscli -b https://skyzh.github.io/mini-lsm -r book -f txt -o > src/sitemap.txt
