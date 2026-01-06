#!/bin/bash

cargo build
rm -rf node_modules
rm -rf frontend/node_modules
pnpm install

