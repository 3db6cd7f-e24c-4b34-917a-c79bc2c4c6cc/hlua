/*
** Library initialization.
** Copyright (C) 2005-2021 Mike Pall. See Copyright Notice in luajit.h
**
** Major parts taken verbatim from the Lua interpreter.
** Copyright (C) 1994-2008 Lua.org, PUC-Rio. See Copyright Notice in lua.h
*/

#define lib_init_c
#define LUA_LIB

#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"

#include "lj_arch.h"

static const luaL_Reg lj_lib_load[] = {
#ifndef WB_DISABLE_LIB_BASE
  { "",			luaopen_base },
#endif
#ifndef WB_DISABLE_LIB_PACKAGE
  { LUA_LOADLIBNAME,	luaopen_package },
#endif
#ifndef WB_DISABLE_LIB_TABLE
  { LUA_TABLIBNAME,	luaopen_table },
#endif
#ifndef WB_DISABLE_LIB_IO
  { LUA_IOLIBNAME,	luaopen_io },
#endif
#ifndef WB_DISABLE_LIB_OS
  { LUA_OSLIBNAME,	luaopen_os },
#endif
#ifndef WB_DISABLE_LIB_STRING
  { LUA_STRLIBNAME,	luaopen_string },
#endif
#ifndef WB_DISABLE_LIB_MATH
  { LUA_MATHLIBNAME,	luaopen_math },
#endif
#ifndef WB_DISABLE_LIB_DEBUG
  { LUA_DBLIBNAME,	luaopen_debug },
#endif
#ifndef WB_DISABLE_LIB_BIT
  { LUA_BITLIBNAME,	luaopen_bit },
#endif
#ifndef WB_DISABLE_LIB_JIT
  { LUA_JITLIBNAME,	luaopen_jit },
#endif
  { NULL,		NULL }
};

static const luaL_Reg lj_lib_preload[] = {
#if LJ_HASFFI
  { LUA_FFILIBNAME,	luaopen_ffi },
#endif
  { NULL,		NULL }
};

LUALIB_API void luaL_openlibs(lua_State *L)
{
  const luaL_Reg *lib;
  for (lib = lj_lib_load; lib->func; lib++) {
    lua_pushcfunction(L, lib->func);
    lua_pushstring(L, lib->name);
    lua_call(L, 1, 0);
  }
  luaL_findtable(L, LUA_REGISTRYINDEX, "_PRELOAD",
		 sizeof(lj_lib_preload)/sizeof(lj_lib_preload[0])-1);
  for (lib = lj_lib_preload; lib->func; lib++) {
    lua_pushcfunction(L, lib->func);
    lua_setfield(L, -2, lib->name);
  }
  lua_pop(L, 1);
}

