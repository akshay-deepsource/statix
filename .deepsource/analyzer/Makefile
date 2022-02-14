build:
	apk update
	cp -r ../../. /app
	apk add lua
	apk add luarocks
	cd /app
	luarocks install luafilesystem
	luarocks install luacheck
	luarocks install lua-path
	luarocks path --bin >> ~/.bash_profile
