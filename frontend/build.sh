#!/bin/sh
node_modules/watchify/bin/cmd.js src/*.ts -p tsify --verbose -o bundle.js
