#!/bin/sh
# Catálogo real da skill montada em $D (o dump do TS lê o kit de $D) e stubs executáveis.
mkdir -p data
cp "$HADES_SKILL_DIR/kit/config.tsv" data/
[ -d bin ] && chmod +x bin/*
exit 0
