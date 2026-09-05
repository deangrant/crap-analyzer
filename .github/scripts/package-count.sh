#!/bin/sh
# Print the number of workspace packages (cargo metadata --no-deps).
set -eu

cargo metadata --no-deps --format-version 1 --offline \
  | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["packages"]))'
