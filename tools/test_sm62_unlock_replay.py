#!/usr/bin/env python3
"""Replay the real host unlock recovery trace through the pinned firmware parser.

Produce trace with ENTROPY_SM62_UNLOCK_TRACE=/absolute/file.json cargo test
sm62_usb_role_change_keeps_browser_profile_and_sends_to_new_selected_owner.
No keyboard is opened; only QMK hardware/time boundaries are substituted.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

p = argparse.ArgumentParser()
p.add_argument('--firmware-source', type=Path, required=True)
p.add_argument('--trace', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
a.output.mkdir(parents=True, exist_ok=True)
root = a.firmware_source.resolve()
source = (root / 'keyboards/ergohaven/hid.c').read_text()
production = source[source.index('#    define EH_APP_LAYOUT_PROTOCOL_VERSION'):source.index('\nhid_data_t *get_hid_data')].rsplit('#endif', 1)[0]
trace = json.loads(a.trace.read_text())
assert trace['initial'] and trace['after_unlock']

def packets(name, values):
    assert values and all(0 < len(v) <= 32 and all(isinstance(x, int) and 0 <= x <= 255 for x in v) for v in values)
    return 'static const uint8_t ' + name + '[][32]={' + ','.join('{' + ','.join(map(str, v)) + '}' for v in values) + '};\n'

header = r'''
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <string.h>
#include <stdio.h>
#include <assert.h>
#define KC_NO 0
#define KC_TRNS 1
static uint32_t now=100,layer_state=1,default_layer_state=1;
static uint32_t timer_read32(void){return now;}
static uint32_t timer_elapsed32(uint32_t t){return now-t;}
static uint8_t get_highest_layer(uint32_t s){return 31-__builtin_clz(s);}
static void layer_clear(void){layer_state=1;}
static uint16_t down_code;
static void vial_keycode_down(uint16_t k){down_code=k;}
static void vial_keycode_up(uint16_t k){(void)k;}
static void vial_keycode_tap(uint16_t k){(void)k;}
static void hid_app_layout_changed_kb(void){}
'''
body = r'''
static void replay(const uint8_t p[][32],size_t n){
 for(size_t i=0;i<n;i++) { uint8_t b[32];memcpy(b,p[i],32);
  if(b[0]>=0xe0&&b[0]<=0xe9) assert(hid_app_layout_process_packet(b,32));
 }
}
int main(void){
 replay(initial,sizeof(initial)/32);
 assert(app_layout_active);
 assert(app_layout_keycodes[0][0]==0x41);
 assert(!strcmp(app_layout_layer_names[0],"Browser keys"));
 now+=2600;hid_app_layout_task();assert(!app_layout_active);
 replay(keepalive,1);assert(!app_layout_active);
 assert(!hid_app_layout_process_keyevent(0,5,true));
 replay(after,sizeof(after)/32);
 assert(app_layout_active);
 assert(app_layout_keycodes[0][0]==0x41);
 assert(!strcmp(app_layout_layer_names[0],"Browser keys"));
 assert(hid_app_layout_process_keyevent(0,5,true));assert(down_code==0x41);
 assert(hid_app_layout_process_keyevent(0,5,false));
 printf("PASS real firmware parser: expired overlay rejects keepalive; real host post-unlock packets restore Browser keys and F8 press/release\n");
}
'''
c = a.output / 'unlock_replay.c'
c.write_text(header + production + packets('initial', trace['initial']) + packets('keepalive', [trace['keepalive']]) + packets('after', trace['after_unlock']) + body)
exe = a.output / 'unlock_replay'
subprocess.run(['gcc', '-std=gnu11', '-O1', '-g', '-Wall', '-Wextra', '-Werror', '-fsanitize=undefined', '-fno-sanitize-recover=all', '-DKEYBOARD_ergohaven_sm62', '-I'+str(root/'keyboards/ergohaven'), str(c), '-o', str(exe)], check=True)
subprocess.run([str(exe)], check=True)
print('firmware hid.c SHA256', hashlib.sha256(source.encode()).hexdigest())
print('host trace SHA256', hashlib.sha256(a.trace.read_bytes()).hexdigest())
