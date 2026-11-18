// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

package main

import (
	"testing"

	tea "charm.land/bubbletea/v2"
)

// The frames of once.json and loop.json, which differ only in looping.
var want = []string{"<\"'>\\\n`${}`", " \\ / \n  -  ", "#{1}#\n#@a #"}

func TestArtPlaysTheFrames(t *testing.T) {
	var model tea.Model = NewArt()
	cmd := model.Init()
	for i := 0; ; i++ {
		if got := model.View().Content; got != want[i%len(want)] {
			t.Fatalf("step %d shows %q", i, got)
		}
		if i == len(want) {
			if !loop {
				t.Fatal("the last frame moved on although the art does not loop")
			}
			return // back on the first frame
		}
		if cmd == nil {
			if loop || i != len(want)-1 {
				t.Fatalf("step %d starts no timer", i)
			}
			return // stays on the last frame
		}
		model, cmd = model.Update(artTick{})
	}
}

func TestQOrCtrlCQuits(t *testing.T) {
	for _, key := range []tea.KeyPressMsg{{Code: 'q', Text: "q"}, {Code: 'c', Mod: tea.ModCtrl}} {
		_, cmd := NewArt().Update(key)
		if cmd == nil {
			t.Fatalf("%s does not quit", key)
		}
		if _, ok := cmd().(tea.QuitMsg); !ok {
			t.Fatalf("%s does not quit", key)
		}
	}
}
