const CF_URL = 'https://europe-west1-calma-498512.cloudfunctions.net/contactForm';

// ===== Institutional form =====
// Guarded: the institutional form only exists on the landing page, not on
// how-it-works.html. Without this check the null deref would halt the rest of
// this script (waitlist + privacy modals) on pages that omit the form.
const instForm = document.getElementById('instForm');
if (instForm) instForm.addEventListener('submit', async (e) => {
  e.preventDefault();
  const status = document.getElementById('instStatus');
  status.textContent = 'Submitting…';
  status.className = 'text-xs text-center text-muted';

  const data = Object.fromEntries(new FormData(e.target));
  data.interest  = [...e.target.querySelectorAll('[name="interest"]:checked')].map(el => el.value);
  data.formType  = 'institutional';

  try {
    const res = await fetch(CF_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    });
    if (res.ok) {
      status.textContent = '✓ Got it. We will reach out within 48h.';
      status.className = 'text-xs text-center text-accent';
      e.target.reset();
    } else {
      status.textContent = 'Something went wrong. Please try again.';
      status.className = 'text-xs text-center text-red-400';
    }
  } catch {
    status.textContent = 'Something went wrong. Please try again.';
    status.className = 'text-xs text-center text-red-400';
  }
});

// ===== Waitlist modal =====
// defer guarantees DOM is ready; DOMContentLoaded not needed
const modal   = document.getElementById('waitlistModal');
const formBox = document.getElementById('waitlistForm');
const success = document.getElementById('waitlistSuccess');
const formEl  = document.getElementById('waitlistFormEl');
const errBox  = document.getElementById('wlError');

const openModal = () => {
  formBox.classList.remove('hidden');
  success.classList.add('hidden');
  if (errBox) { errBox.classList.add('hidden'); errBox.textContent = ''; }
  modal.classList.remove('hidden');
  setTimeout(() => { const f = document.getElementById('wlEmail'); if (f) f.focus(); }, 50);
};
const closeModal = () => { modal.classList.add('hidden'); if (formEl) formEl.reset(); };

document.querySelectorAll('[data-waitlist]').forEach(b => b.addEventListener('click', (e) => { e.preventDefault(); openModal(); }));
modal.querySelectorAll('[data-waitlist-close]').forEach(el => el.addEventListener('click', closeModal));
document.addEventListener('keydown', (e) => {
  // don't close the waitlist while the privacy modal is stacked on top of it
  if (e.key === 'Escape' && !modal.classList.contains('hidden') && privacyModal.classList.contains('hidden')) closeModal();
});
// prevent hero pager from advancing on scroll while the modal is open
['wheel', 'touchmove'].forEach(ev => modal.addEventListener(ev, (e) => e.stopPropagation(), { passive: true }));

// ===== Privacy policy modal (stacks above the waitlist modal) =====
const privacyModal = document.getElementById('privacyModal');
const openPrivacy  = () => privacyModal.classList.remove('hidden');
const closePrivacy = () => privacyModal.classList.add('hidden');
document.querySelectorAll('[data-privacy-open]').forEach(el => el.addEventListener('click', (e) => { e.preventDefault(); openPrivacy(); }));
privacyModal.querySelectorAll('[data-privacy-close]').forEach(el => el.addEventListener('click', closePrivacy));
document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && !privacyModal.classList.contains('hidden')) closePrivacy(); });
['wheel', 'touchmove'].forEach(ev => privacyModal.addEventListener(ev, (e) => e.stopPropagation(), { passive: true }));

// ===== Deep link: open the modal when arriving at #waitlist =====
const openFromHash = () => {
  if (location.hash.toLowerCase() === '#waitlist') openModal();
};
openFromHash();                                      // on initial load
window.addEventListener('hashchange', openFromHash); // if navigated to while loaded

// reveal the free-text field when "Other" is ticked
const other     = document.getElementById('intOther');
const otherText = document.getElementById('intOtherText');
if (other && otherText) other.addEventListener('change', () => otherText.classList.toggle('hidden', !other.checked));

// ===== Waitlist form submit =====
if (formEl) formEl.addEventListener('submit', async (e) => {
  e.preventDefault();
  const wallet = document.getElementById('wlWallet').value.trim();
  if (!/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(wallet)) {
    if (errBox) { errBox.textContent = "That doesn't look like a Solana address — please double-check."; errBox.classList.remove('hidden'); }
    return;
  }
  if (errBox) errBox.classList.add('hidden');

  try {
    const res = await fetch(CF_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        email: document.getElementById('wlEmail').value.trim(),
        wallet,
        formType: 'waitlist',
      }),
    });
    if (res.ok) {
      formBox.classList.add('hidden');
      success.classList.remove('hidden');
    } else {
      if (errBox) { errBox.textContent = 'Something went wrong. Please try again.'; errBox.classList.remove('hidden'); }
    }
  } catch {
    if (errBox) { errBox.textContent = 'Something went wrong. Please try again.'; errBox.classList.remove('hidden'); }
  }
});
