#!/bin/bash

echo "Listing /usr/src/app contents:"
ls -l /usr/src/app

echo "Environment Variables:"
env

echo "Attempting to run combinedwallettest:"
./usr/src/app/combinedwallettest > output.log 2>&1 &

# Wait for the application to start
sleep 10

echo "Contents of output.log:"
cat output.log

echo "Checking if the application is listening on port 8080:"
if command -v netstat > /dev/null; then
    netstat -tuln | grep 8080
elif command -v ss > /dev/null; then
    ss -tuln | grep 8080
else
    echo "Neither netstat nor ss command is available"
fi

echo "Attempting to make a GET request to the application with API key:"
curl -v -X GET http://localhost:8080/decrypt_keys -H "Content-Type: application/json" -d '{"api_key": "7c393074-c38c-4ce5-9d27-ecfee023b00a"}'
