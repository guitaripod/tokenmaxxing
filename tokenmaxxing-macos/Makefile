APP := Tokenmaxxing
BUILD_DIR := .build/release
APP_BUNDLE := $(APP).app
CONFIG := release

.PHONY: all build bundle sign run install clean

all: sign

build:
	swift build -c $(CONFIG)

bundle: build
	rm -rf $(APP_BUNDLE)
	mkdir -p $(APP_BUNDLE)/Contents/MacOS
	mkdir -p $(APP_BUNDLE)/Contents/Resources
	cp $(BUILD_DIR)/$(APP) $(APP_BUNDLE)/Contents/MacOS/$(APP)
	cp Resources/Info.plist $(APP_BUNDLE)/Contents/Info.plist

sign: bundle
	codesign --force --sign - $(APP_BUNDLE)

run: sign
	open $(APP_BUNDLE)

install: sign
	rm -rf /Applications/$(APP_BUNDLE)
	cp -R $(APP_BUNDLE) /Applications/
	open /Applications/$(APP_BUNDLE)

clean:
	rm -rf .build $(APP_BUNDLE)
