#!/bin/bash
set -e

if [ -z "$TELEGRAM_TOKEN" ]; then
  echo "Error: TELEGRAM_TOKEN is not set"
  echo "Usage: TELEGRAM_TOKEN=your_token TELEGRAM_CHAT_ID=your_chat_id ./deploy.sh"
  exit 1
fi

if [ -z "$TELEGRAM_CHAT_ID" ]; then
  echo "Error: TELEGRAM_CHAT_ID is not set"
  echo "Usage: TELEGRAM_TOKEN=your_token TELEGRAM_CHAT_ID=your_chat_id ./deploy.sh"
  exit 1
fi

gcloud functions deploy contactForm \
  --project calma-functions \
  --runtime nodejs24 \
  --trigger-http \
  --allow-unauthenticated \
  --region europe-west1 \
  --source . \
  --set-env-vars TELEGRAM_TOKEN=$TELEGRAM_TOKEN,TELEGRAM_CHAT_ID=$TELEGRAM_CHAT_ID
