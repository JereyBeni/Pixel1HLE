//! Embedded sailfish profile so Android / missing-file boots still work

pub const SAILFISH_YAML: &str = r#"
device:
  codename: sailfish
  model: "Pixel"
  manufacturer: "Google"
  android_version: "7.1.2"
  api_level: 25
  abi: arm64-v8a

hardware:
  display:
    width: 1280
    height: 720
    ppi: 441
    refresh_hz: 60
    present: true
  touch:
    multi_touch: true
    max_pointers: 10
    present: true
  audio:
    present: true
    jack_3_5mm: true
    speaker: true
  storage:
    internal_gb: 32
    present: true
  battery:
    present: true
    capacity_mah: 2770
    force_full_and_charging: true
  camera:
    present: false
  fingerprint:
    present: false
  nfc:
    present: false
  bluetooth:
    present: false
  wifi:
    present: true
  gps:
    present: false

permissions:
  allowlist:
    - android.permission.INTERNET
    - android.permission.ACCESS_NETWORK_STATE
    - android.permission.WAKE_LOCK
    - android.permission.VIBRATE

binder:
  services:
    WINDOW_SERVICE: present
    AUDIO_SERVICE: present
    LOCATION_SERVICE: absent
    CAMERA_SERVICE: absent
    FINGERPRINT_SERVICE: absent
    POWER_SERVICE: present
    BATTERY_SERVICE: present
"#;
