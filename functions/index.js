const https = require('https');

const TELEGRAM_TOKEN = process.env.TELEGRAM_TOKEN;
const TELEGRAM_CHAT_ID = process.env.TELEGRAM_CHAT_ID;

exports.contactForm = (req, res) => {
  res.set('Access-Control-Allow-Origin', '*');
  res.set('Access-Control-Allow-Methods', 'POST, OPTIONS');
  res.set('Access-Control-Allow-Headers', 'Content-Type');

  if (req.method === 'OPTIONS') {
    res.status(204).send('');
    return;
  }

  if (req.method !== 'POST') {
    res.status(405).send('Method Not Allowed');
    return;
  }

  const { email, org, interest, msg } = req.body;

  if (!email || !org) {
    res.status(400).json({ error: 'Missing required fields' });
    return;
  }

  const interests = Array.isArray(interest) ? interest.join(', ') : (interest || '—');

  const text = [
    '📩 *New institutional inquiry*',
    `*Email:* ${email}`,
    `*Org:* ${org}`,
    `*Interest:* ${interests}`,
    `*Message:* ${msg || '—'}`,
  ].join('\n');

  const payload = JSON.stringify({
    chat_id: TELEGRAM_CHAT_ID,
    text,
    parse_mode: 'Markdown',
  });

  const options = {
    hostname: 'api.telegram.org',
    path: `/bot${TELEGRAM_TOKEN}/sendMessage`,
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Content-Length': Buffer.byteLength(payload),
    },
  };

  const telegramReq = https.request(options, (telegramRes) => {
    let data = '';
    telegramRes.on('data', (chunk) => { data += chunk; });
    telegramRes.on('end', () => {
      const parsed = JSON.parse(data);
      if (parsed.ok) {
        res.status(200).json({ success: true });
      } else {
        res.status(500).json({ error: 'Telegram error', details: parsed });
      }
    });
  });

  telegramReq.on('error', (e) => {
    res.status(500).json({ error: e.message });
  });

  telegramReq.write(payload);
  telegramReq.end();
};
