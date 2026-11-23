#!/bin/sh
# Repo de fixture em ~/repo com remote do GitHub fictício (o loop de revisões acha o slug pelo remote).
sh "$(dirname "$0")/setup-config.sh"
git init -q repo && git -C repo remote add origin https://github.com/acme/demo.git
exit 0
