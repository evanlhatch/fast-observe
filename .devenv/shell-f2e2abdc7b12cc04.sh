if [ -n "$PS1" ] && [ -e $HOME/.bashrc ]; then
    source $HOME/.bashrc;
fi

shopt -u expand_aliases
PATH=${PATH:-}
nix_saved_PATH="$PATH"
XDG_DATA_DIRS=${XDG_DATA_DIRS:-}
nix_saved_XDG_DATA_DIRS="$XDG_DATA_DIRS"
SIZE_FOR_BUILD='size'
export SIZE_FOR_BUILD
declare -a envBuildHostHooks=('ccWrapper_addCVars' 'bintoolsWrapper_addLDVars' )
propagatedBuildInputs=''
export propagatedBuildInputs
declare -a envTargetTargetHooks=()
XDG_DATA_DIRS='/nix/store/64w7zfk9dd7b0q2mc4520z10l8cxq7cy-bacon-3.24.0/share:/nix/store/21cakhvvk291d2r27aqim6djdw0zlkik-bash-interactive-5.3p15/share:/nix/store/5vviw6b8frj2xs0j34d7dm9rnszwjplf-rust-analyzer-preview-1.100.0-nightly-2026-08-17-aarch64-unknown-linux-gnu/share:/nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17/share:/nix/store/wadg45zgfyi9a4dcm5fy3qvlas76cr2z-gnumake-4.4.1/share:/nix/store/47viyl08bzj7863dvcr8j08wcaiwjaf4-pkg-config-wrapper-0.29.2/share:/nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/share:/nix/store/jpb6s00z3fhz0nljlwv2zr5577r5j0pa-jujutsu-0.44.0/share:/nix/store/r6hjn3p91594n85ww9jxs9k230wmwqhh-just-1.57.0/share:/nix/store/19wxr8hcynk6025kq7yk3vmv6j2yjz21-ripgrep-15.2.0/share:/nix/store/xayxls1146z501jzarabbpjq1xy6bpda-prek-0.4.10/share:/nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped/share:/nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped/share:/nix/store/8n1hr4cx9wb6i52jcnrsslz8wiz7r9x7-patchelf-0.15.2/share'
export XDG_DATA_DIRS
OSTYPE='linux-gnu'
NIX_CC_WRAPPER_TARGET_HOST_aarch64_unknown_linux_gnu='1'
export NIX_CC_WRAPPER_TARGET_HOST_aarch64_unknown_linux_gnu
NIX_STORE='/nix/store'
export NIX_STORE
_substituteStream_has_warned_replace_deprecation='false'
DEVENV_DOTFILE='/home/evan/fast-observe/.devenv'
export DEVENV_DOTFILE
propagatedNativeBuildInputs=''
export propagatedNativeBuildInputs
declare -a preFixupHooks=('_moveToShare' '_multioutDocs' '_multioutDevs' )
RUSTDOCFLAGS='-C linker-features=-lld -C link-arg=-B/nix/store/3f5rhrsg2j4x0fhvvh8fir6qf37r56pa-wild-unwrapped-wrapper-0.9.0/bin -Z unstable-options'
export RUSTDOCFLAGS
LINENO='79'
PKG_CONFIG_PATH='/nix/store/8py4d4r6izw1cwpn52z3w2w9whgc859i-bash-interactive-5.3p15-dev/lib/pkgconfig:/nix/store/b2vxsljfjb0n2z735q2y8x0yix1dhzi4-valgrind-3.27.1-dev/lib/pkgconfig'
export PKG_CONFIG_PATH
AR_FOR_BUILD='ar'
export AR_FOR_BUILD
IN_NIX_SHELL='impure'
export IN_NIX_SHELL
out='/nix/store/yn01591sk5z0z5z595fyz57iq62cfs92-devenv-shell-env'
export out
HOSTTYPE='aarch64'
depsHostHostPropagated=''
export depsHostHostPropagated
patches=''
export patches
declare -a envHostHostHooks=('bintoolsWrapper_addLDVars' 'ccWrapper_addCVars' 'bintoolsWrapper_addLDVars' 'ccWrapper_addCVars' 'bintoolsWrapper_addLDVars' 'pkgConfigWrapper_addPkgConfigPath' )
declare -a pkgsHostTarget=()
doCheck=''
export doCheck
declare -a fixupOutputHooks=('if [ -z "${dontPatchELF-}" ]; then patchELF "$prefix"; fi' 'if [[ -z "${noAuditTmpdir-}" && -e "$prefix" ]]; then auditTmpdir "$prefix"; fi' 'if [ -z "${dontGzipMan-}" ]; then compressManPages "$prefix"; fi' '_moveLib64' '_moveSbin' '_moveSystemdUserUnits' 'patchShebangsAuto' '_pruneLibtoolFiles' '_doStrip' )
HOST_PATH='/nix/store/pffmrhqna5mmi75z0213h3s3vvxdqala-compiler-rt-libc-21.1.8/bin:/nix/store/p79fmimbb698sv4c135kbdwlhjcqpd3p-coreutils-9.11/bin:/nix/store/2mj9fbglfqvfizz274hrd8bgvlwv1n0y-findutils-4.11.0/bin:/nix/store/mavadygrg9dx0x4b7rhna9njqlyiplga-diffutils-3.12/bin:/nix/store/jnsbsffmr6zk1y8zic51z4r5ik989gdq-gnused-4.10/bin:/nix/store/cspav3bpfmcsp55lspmc5nvbvyyly1ag-gnugrep-3.12/bin:/nix/store/k03bq9lmv9ssyklfhl852xr8d2isydjd-gawk-5.4.1/bin:/nix/store/8z1grz46krqzzlqbicda7kq383565l8a-gnutar-1.35/bin:/nix/store/wmy40hkad9p9wswn4r8gaqkxsvaby52h-gzip-1.14/bin:/nix/store/phj68a4my78qjjkkqz8mnxkxac7jd5ly-bzip2-1.0.8-bin/bin:/nix/store/mp5ca9knfc0ldra1asdv7xqsm4jq1lmd-gnumake-4.4.1/bin:/nix/store/rzpv2vlq1zmhjqjinjgff1v83nw0b3hz-bash-5.3p15/bin:/nix/store/h3b19sgsmz4vf9ssadhigfnxy9iy1rkr-patch-2.8/bin:/nix/store/i672h9rp9lm8rngn8r6gf8la8z691mkx-xz-5.8.3-bin/bin:/nix/store/y7vsgfs9a32wlhibfmyvwvgvrl737cfv-file-5.48/bin'
export HOST_PATH
OBJDUMP='objdump'
export OBJDUMP
OBJDUMP_FOR_BUILD='objdump'
export OBJDUMP_FOR_BUILD
NIX_BUILD_CORES='4'
export NIX_BUILD_CORES
declare -a pkgsBuildBuild=('/nix/store/ha244r9jv8a3qyj7m7gr770a00i77vfc-gcc-wrapper-15.3.0' '/nix/store/i2sry2igh6r2vadphmkd9q0ibkkya12n-binutils-wrapper-2.46' )
defaultBuildInputs=''
preConfigurePhases=' updateAutotoolsGnuConfigScriptsPhase'
stdenv='/nix/store/5y0xpxiqwpbh2myj7rq97vpacazpgg0g-stdenv-linux'
export stdenv
DEVENV_ROOT='/home/evan/fast-observe'
export DEVENV_ROOT
NIX_CFLAGS_COMPILE=' -frandom-seed=yn01591sk5 -isystem /nix/store/8py4d4r6izw1cwpn52z3w2w9whgc859i-bash-interactive-5.3p15-dev/include -isystem /nix/store/8py4d4r6izw1cwpn52z3w2w9whgc859i-bash-interactive-5.3p15-dev/include -isystem /nix/store/wadg45zgfyi9a4dcm5fy3qvlas76cr2z-gnumake-4.4.1/include -isystem /nix/store/wadg45zgfyi9a4dcm5fy3qvlas76cr2z-gnumake-4.4.1/include -isystem /nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/include -isystem /nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/include -isystem /nix/store/b2vxsljfjb0n2z735q2y8x0yix1dhzi4-valgrind-3.27.1-dev/include -isystem /nix/store/b2vxsljfjb0n2z735q2y8x0yix1dhzi4-valgrind-3.27.1-dev/include -isystem /nix/store/j0zsbhkcirm6wcy1f3dj844p6vdj96y6-compiler-rt-libc-21.1.8-dev/include -isystem /nix/store/j0zsbhkcirm6wcy1f3dj844p6vdj96y6-compiler-rt-libc-21.1.8-dev/include -isystem /nix/store/8py4d4r6izw1cwpn52z3w2w9whgc859i-bash-interactive-5.3p15-dev/include -isystem /nix/store/8py4d4r6izw1cwpn52z3w2w9whgc859i-bash-interactive-5.3p15-dev/include -isystem /nix/store/wadg45zgfyi9a4dcm5fy3qvlas76cr2z-gnumake-4.4.1/include -isystem /nix/store/wadg45zgfyi9a4dcm5fy3qvlas76cr2z-gnumake-4.4.1/include -isystem /nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/include -isystem /nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/include -isystem /nix/store/b2vxsljfjb0n2z735q2y8x0yix1dhzi4-valgrind-3.27.1-dev/include -isystem /nix/store/b2vxsljfjb0n2z735q2y8x0yix1dhzi4-valgrind-3.27.1-dev/include -isystem /nix/store/j0zsbhkcirm6wcy1f3dj844p6vdj96y6-compiler-rt-libc-21.1.8-dev/include -isystem /nix/store/j0zsbhkcirm6wcy1f3dj844p6vdj96y6-compiler-rt-libc-21.1.8-dev/include'
export NIX_CFLAGS_COMPILE
declare -a envBuildBuildHooks=('ccWrapper_addCVars' 'bintoolsWrapper_addLDVars' )
NIX_CC_WRAPPER_TARGET_BUILD_aarch64_unknown_linux_gnu='1'
export NIX_CC_WRAPPER_TARGET_BUILD_aarch64_unknown_linux_gnu
PS4='+ '
buildPhase='{ echo "------------------------------------------------------------";
  echo " WARNING: the existence of this path is not guaranteed.";
  echo " It is an internal implementation detail for pkgs.mkShell.";
  echo "------------------------------------------------------------";
  echo;
  # Record all build inputs as runtime dependencies
  export;
} >> "$out"
'
export buildPhase
checkPhase='justCheckPhase'
declare -a envHostTargetHooks=('bintoolsWrapper_addLDVars' 'ccWrapper_addCVars' 'bintoolsWrapper_addLDVars' 'ccWrapper_addCVars' 'bintoolsWrapper_addLDVars' 'pkgConfigWrapper_addPkgConfigPath' )
declare -a pkgsBuildTarget=()
outputInclude='out'
declare -a propagatedBuildDepFiles=('propagated-build-build-deps' 'propagated-native-build-inputs' 'propagated-build-target-deps' )
declare -a preConfigureHooks=('_multioutConfig' )
outputDevman='out'
__structuredAttrs=''
export __structuredAttrs
AS='as'
export AS
outputBin='out'
NIX_ENFORCE_NO_NATIVE='1'
export NIX_ENFORCE_NO_NATIVE
outputDev='out'
initialPath='/nix/store/p79fmimbb698sv4c135kbdwlhjcqpd3p-coreutils-9.11 /nix/store/2mj9fbglfqvfizz274hrd8bgvlwv1n0y-findutils-4.11.0 /nix/store/mavadygrg9dx0x4b7rhna9njqlyiplga-diffutils-3.12 /nix/store/jnsbsffmr6zk1y8zic51z4r5ik989gdq-gnused-4.10 /nix/store/cspav3bpfmcsp55lspmc5nvbvyyly1ag-gnugrep-3.12 /nix/store/k03bq9lmv9ssyklfhl852xr8d2isydjd-gawk-5.4.1 /nix/store/8z1grz46krqzzlqbicda7kq383565l8a-gnutar-1.35 /nix/store/wmy40hkad9p9wswn4r8gaqkxsvaby52h-gzip-1.14 /nix/store/phj68a4my78qjjkkqz8mnxkxac7jd5ly-bzip2-1.0.8-bin /nix/store/mp5ca9knfc0ldra1asdv7xqsm4jq1lmd-gnumake-4.4.1 /nix/store/rzpv2vlq1zmhjqjinjgff1v83nw0b3hz-bash-5.3p15 /nix/store/h3b19sgsmz4vf9ssadhigfnxy9iy1rkr-patch-2.8 /nix/store/i672h9rp9lm8rngn8r6gf8la8z691mkx-xz-5.8.3-bin /nix/store/y7vsgfs9a32wlhibfmyvwvgvrl737cfv-file-5.48'
RUSTFLAGS='-C linker-features=-lld -C link-arg=-B/nix/store/3f5rhrsg2j4x0fhvvh8fir6qf37r56pa-wild-unwrapped-wrapper-0.9.0/bin -Z unstable-options -C lto=off -C codegen-units=256 -C opt-level=1 -Zshare-generics=y -Zthreads=8 -Zpolonius=next -Zinline-mir -Zub-checks -C target-cpu=native'
export RUSTFLAGS
READELF_FOR_BUILD='readelf'
export READELF_FOR_BUILD
phases='buildPhase'
export phases
depsBuildBuild=''
export depsBuildBuild
CC_FOR_BUILD='gcc'
export CC_FOR_BUILD
NIX_CC='/nix/store/ha244r9jv8a3qyj7m7gr770a00i77vfc-gcc-wrapper-15.3.0'
export NIX_CC
depsBuildTargetPropagated=''
export depsBuildTargetPropagated
prefix='/nix/store/yn01591sk5z0z5z595fyz57iq62cfs92-devenv-shell-env'
NIX_BINTOOLS_WRAPPER_TARGET_HOST_aarch64_unknown_linux_gnu='1'
export NIX_BINTOOLS_WRAPPER_TARGET_HOST_aarch64_unknown_linux_gnu
declare -a pkgsTargetTarget=()
NIX_LDFLAGS_FOR_BUILD=' -L/nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17/lib -L/nix/store/nsz7j3qfpidrdbqqppk912jic5isl8n2-clang-tools-21.1.8/lib -L/nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/lib -L/nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped/lib -L/nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped/lib -L/nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17/lib -L/nix/store/nsz7j3qfpidrdbqqppk912jic5isl8n2-clang-tools-21.1.8/lib -L/nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/lib -L/nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped/lib -L/nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped/lib -L/nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17/lib -L/nix/store/nsz7j3qfpidrdbqqppk912jic5isl8n2-clang-tools-21.1.8/lib -L/nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/lib -L/nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped/lib -L/nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped/lib'
export NIX_LDFLAGS_FOR_BUILD
depsTargetTarget=''
export depsTargetTarget
outputs='out'
export outputs
buildInputs=''
export buildInputs
RUSTFLAGS_FIX='-C lto=off -C codegen-units=256 -C opt-level=1 -Zshare-generics=y -C target-cpu=native'
export RUSTFLAGS_FIX
shellHook='


# Override temp directories that stdenv set to NIX_BUILD_TOP.
# Only reset those that still point to the Nix build dir; leave
# any user/CI-supplied value intact.
for var in TMP TMPDIR TEMP TEMPDIR; do
  if [ -n "${!var-}" ] && [ "${!var}" = "${NIX_BUILD_TOP-}" ]; then
    export "$var"=/tmp
  fi
done
if [ -n "${NIX_BUILD_TOP-}" ]; then
  unset NIX_BUILD_TOP
fi

# set path to locales on non-NixOS Linux hosts
if [ -z "${LOCALE_ARCHIVE-}" ]; then
  export LOCALE_ARCHIVE=/nix/store/kzwaqipblsz0mb6wdy4pnqamfij87gjj-glibc-locales-2.42-67/lib/locale/locale-archive
fi


# Make `man <tool>` find the man pages of packages in the profile.
export MANPATH="$DEVENV_PROFILE/share/man:${MANPATH:+$MANPATH:}"

# direnv helper
if [ ! type -p direnv &>/dev/null && -f .envrc ]; then
  echo "An .envrc file was detected, but the direnv command is not installed."
  echo "To use this configuration, please install direnv: https://direnv.net/docs/installation.html"
fi

mkdir -p "$DEVENV_STATE"
if [ ! -L "$DEVENV_DOTFILE/profile" ] || [ "$(/nix/store/p79fmimbb698sv4c135kbdwlhjcqpd3p-coreutils-9.11/bin/readlink $DEVENV_DOTFILE/profile)" != "/nix/store/v8s7wsx40bcdp2pc9iwxw9qr2h090h0g-devenv-profile" ]
then
  ln -snf /nix/store/v8s7wsx40bcdp2pc9iwxw9qr2h090h0g-devenv-profile "$DEVENV_DOTFILE/profile"
fi
unset HOST_PATH NIX_BUILD_CORES __structuredAttrs buildInputs buildPhase builder depsBuildBuild depsBuildBuildPropagated depsBuildTarget depsBuildTargetPropagated depsHostHost depsHostHostPropagated depsTargetTarget depsTargetTargetPropagated dontAddDisableDepTrack doCheck doInstallCheck nativeBuildInputs out outputs patches phases preferLocalBuild propagatedBuildInputs propagatedNativeBuildInputs shell shellHook stdenv strictDeps

mkdir -p /run/user/1000/devenv-707ea10
ln -snf /run/user/1000/devenv-707ea10 /home/evan/fast-observe/.devenv/run

export CARGO_BUILD_JOBS=$(($(nproc) - 1))
mkdir -p "/home/evan/fast-observe/.devenv/state/coverage/profiles"
mkdir -p "/home/evan/fast-observe/.devenv/state/bolero-corpus"

echo "🦀 fast-observe Rust dev environment ready"


export CARGO_INSTALL_ROOT=$(/nix/store/p79fmimbb698sv4c135kbdwlhjcqpd3p-coreutils-9.11/bin/realpath --no-symlinks /home/evan/fast-observe/.devenv/state/cargo-install)
export PATH="$PATH:$CARGO_INSTALL_ROOT/bin"
export PATH="$PATH:${CARGO_HOME:-$HOME/.cargo}/bin"

echo "✨ devenv 2.2.1 is out of date. Please update to 2.2.2: https://devenv.sh/getting-started/#installation" >&2


# Check whether the direnv integration is out of date.
{
  if [[ ":${DIRENV_ACTIVE-}:" == *":/home/evan/fast-observe:"* ]]; then
    if [[ ! "${DEVENV_NO_DIRENVRC_OUTDATED_WARNING-}" == 1 && ! "${DEVENV_DIRENVRC_ROLLING_UPGRADE-}" == 1 ]]; then
      if [[ ${DEVENV_DIRENVRC_VERSION:-0} -lt 2 ]]; then
        direnv_line=$(grep --color=never -E "source_url.*cachix/devenv" .envrc || echo "")

        echo "✨ The direnv integration in your .envrc is out of date."
        echo ""
        echo -n "RECOMMENDED: devenv can now auto-upgrade the direnv integration. "
        if [[ -n "$direnv_line" ]]; then
          echo "To enable this feature, replace the following line in your .envrc:"
          echo ""
          echo "  $direnv_line"
          echo ""
          echo "with:"
          echo ""
          echo "  eval \"\$(devenv direnvrc)\""
        else
          echo "To enable this feature, replace the \`source_url\` line that fetches the direnvrc integration in your .envrc with:"
          echo ""
          echo "  eval \"$(devenv direnvrc)\""
        fi
        echo ""
          echo "If you prefer to continue managing the integration manually, follow the upgrade instructions at https://devenv.sh/integrations/direnv/."
          echo ""
          echo "To disable this message:"
          echo ""
          echo "  Add the following environment to your .envrc before \`use devenv\`:"
          echo ""
          echo "    export DEVENV_NO_DIRENVRC_OUTDATED_WARNING=1"
          echo ""
          echo "  Or set the following option in your devenv configuration:"
          echo ""
          echo "    devenv.warnOnNewVersion = false;"
          echo ""
      fi
    fi
  fi
} >&2

echo "fast-observe devenv ready"

mkdir -p "$PREK_HOME"

'
export shellHook
declare -a pkgsHostHost=('/nix/store/j0zsbhkcirm6wcy1f3dj844p6vdj96y6-compiler-rt-libc-21.1.8-dev' '/nix/store/pffmrhqna5mmi75z0213h3s3vvxdqala-compiler-rt-libc-21.1.8' )
strictDeps=''
export strictDeps
DEVENV_TASK_FILE='/nix/store/6jqx9zbj2xrci0n95ma3dni6kqzxny77-tasks.json'
export DEVENV_TASK_FILE
NIX_BINTOOLS='/nix/store/i2sry2igh6r2vadphmkd9q0ibkkya12n-binutils-wrapper-2.46'
export NIX_BINTOOLS
defaultNativeBuildInputs='/nix/store/8n1hr4cx9wb6i52jcnrsslz8wiz7r9x7-patchelf-0.15.2 /nix/store/mxxl2xv4dfn002bqydyv85im2aa2rlgg-update-autotools-gnu-config-scripts-hook /nix/store/0y5xmdb7qfvimjwbq7ibg1xdgkgjwqng-no-broken-symlinks.sh /nix/store/cv1d7p48379km6a85h4zp6kr86brh32q-audit-tmpdir.sh /nix/store/85clx3b0xkdf58jn161iy80y5223ilbi-compress-man-pages.sh /nix/store/p3l1a5y7nllfyrjn2krlwgcc3z0cd3fq-make-symlinks-relative.sh /nix/store/5yzw0vhkyszf2d179m0qfkgxmp5wjjx4-move-docs.sh /nix/store/fyaryjvghbkpfnsyw97hb3lyb37s1pd6-move-lib64.sh /nix/store/kd4xwxjpjxi71jkm6ka0np72if9rm3y0-move-sbin.sh /nix/store/pag6l61paj1dc9sv15l7bm5c17xn5kyk-move-systemd-user-units.sh /nix/store/cmzya9irvxzlkh7lfy6i82gbp0saxqj3-multiple-outputs.sh /nix/store/x8c40nfigps493a07sdr2pm5s9j1cdc0-patch-shebangs.sh /nix/store/cickvswrvann041nqxb0rxilc46svw1n-prune-libtool-files.sh /nix/store/xyff06pkhki3qy1ls77w10s0v79c9il0-reproducible-builds.sh /nix/store/z7k98578dfzi6l3hsvbivzm7hfqlk0zc-set-source-date-epoch-to-latest.sh /nix/store/pilsssjjdxvdphlg2h19p0bfx5q0jzkn-strip.sh /nix/store/ha244r9jv8a3qyj7m7gr770a00i77vfc-gcc-wrapper-15.3.0'
STRIP_FOR_BUILD='strip'
export STRIP_FOR_BUILD
PKG_CONFIG='pkg-config'
export PKG_CONFIG
cmakeFlags=''
export cmakeFlags
CC='gcc'
export CC
NIX_CFLAGS_COMPILE_FOR_BUILD=' -isystem /nix/store/8py4d4r6izw1cwpn52z3w2w9whgc859i-bash-interactive-5.3p15-dev/include -isystem /nix/store/wadg45zgfyi9a4dcm5fy3qvlas76cr2z-gnumake-4.4.1/include -isystem /nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/include -isystem /nix/store/b2vxsljfjb0n2z735q2y8x0yix1dhzi4-valgrind-3.27.1-dev/include -isystem /nix/store/j0zsbhkcirm6wcy1f3dj844p6vdj96y6-compiler-rt-libc-21.1.8-dev/include -isystem /nix/store/8py4d4r6izw1cwpn52z3w2w9whgc859i-bash-interactive-5.3p15-dev/include -isystem /nix/store/wadg45zgfyi9a4dcm5fy3qvlas76cr2z-gnumake-4.4.1/include -isystem /nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/include -isystem /nix/store/b2vxsljfjb0n2z735q2y8x0yix1dhzi4-valgrind-3.27.1-dev/include -isystem /nix/store/j0zsbhkcirm6wcy1f3dj844p6vdj96y6-compiler-rt-libc-21.1.8-dev/include -isystem /nix/store/8py4d4r6izw1cwpn52z3w2w9whgc859i-bash-interactive-5.3p15-dev/include -isystem /nix/store/wadg45zgfyi9a4dcm5fy3qvlas76cr2z-gnumake-4.4.1/include -isystem /nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/include -isystem /nix/store/b2vxsljfjb0n2z735q2y8x0yix1dhzi4-valgrind-3.27.1-dev/include -isystem /nix/store/j0zsbhkcirm6wcy1f3dj844p6vdj96y6-compiler-rt-libc-21.1.8-dev/include'
export NIX_CFLAGS_COMPILE_FOR_BUILD
depsBuildTarget=''
export depsBuildTarget
outputMan='out'
CXX='g++'
export CXX
DEVENV_TASKS=''
export DEVENV_TASKS
OBJCOPY_FOR_BUILD='objcopy'
export OBJCOPY_FOR_BUILD
mesonFlags=''
export mesonFlags
NIX_BINTOOLS_FOR_BUILD='/nix/store/i2sry2igh6r2vadphmkd9q0ibkkya12n-binutils-wrapper-2.46'
export NIX_BINTOOLS_FOR_BUILD
NIX_LDFLAGS='-rpath /nix/store/yn01591sk5z0z5z595fyz57iq62cfs92-devenv-shell-env/lib  -L/nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17/lib -L/nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17/lib -L/nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17/lib -L/nix/store/nsz7j3qfpidrdbqqppk912jic5isl8n2-clang-tools-21.1.8/lib -L/nix/store/nsz7j3qfpidrdbqqppk912jic5isl8n2-clang-tools-21.1.8/lib -L/nix/store/nsz7j3qfpidrdbqqppk912jic5isl8n2-clang-tools-21.1.8/lib -L/nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/lib -L/nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/lib -L/nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/lib -L/nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped/lib -L/nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped/lib -L/nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped/lib -L/nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped/lib -L/nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped/lib -L/nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped/lib -L/nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17/lib -L/nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17/lib -L/nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17/lib -L/nix/store/nsz7j3qfpidrdbqqppk912jic5isl8n2-clang-tools-21.1.8/lib -L/nix/store/nsz7j3qfpidrdbqqppk912jic5isl8n2-clang-tools-21.1.8/lib -L/nix/store/nsz7j3qfpidrdbqqppk912jic5isl8n2-clang-tools-21.1.8/lib -L/nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/lib -L/nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/lib -L/nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/lib -L/nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped/lib -L/nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped/lib -L/nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped/lib -L/nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped/lib -L/nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped/lib -L/nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped/lib'
export NIX_LDFLAGS
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER='/nix/store/q376w2vgpkj7xi0lld4fy27vxk04dfp9-clang-wrapper-21.1.8/bin/clang'
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER
NIX_PKG_CONFIG_WRAPPER_TARGET_HOST_aarch64_unknown_linux_gnu='1'
export NIX_PKG_CONFIG_WRAPPER_TARGET_HOST_aarch64_unknown_linux_gnu
STRINGS='strings'
export STRINGS
depsTargetTargetPropagated=''
export depsTargetTargetPropagated
outputLib='out'
NIX_HARDENING_ENABLE='bindnow format fortify fortify3 libcxxhardeningfast pic relro stackclashprotection stackprotector strictflexarrays1 strictoverflow zerocallusedregs'
export NIX_HARDENING_ENABLE
NIX_NO_SELF_RPATH='1'
SIZE='size'
export SIZE
NM='nm'
export NM
OLDPWD=''
export OLDPWD
PREK_HOME='/home/evan/fast-observe/.devenv/state/prek'
export PREK_HOME
installPhase='justInstallPhase'
CONFIG_SHELL='/nix/store/rzpv2vlq1zmhjqjinjgff1v83nw0b3hz-bash-5.3p15/bin/bash'
export CONFIG_SHELL
pkg='/nix/store/ha244r9jv8a3qyj7m7gr770a00i77vfc-gcc-wrapper-15.3.0'
outputDoc='out'
configureFlags=''
export configureFlags
OPTERR='1'
outputInfo='out'
BASH='/nix/store/rzpv2vlq1zmhjqjinjgff1v83nw0b3hz-bash-5.3p15/bin/bash'
depsHostHost=''
export depsHostHost
LD='ld'
export LD
outputDevdoc='REMOVE'
PATH='/nix/store/ha244r9jv8a3qyj7m7gr770a00i77vfc-gcc-wrapper-15.3.0/bin:/nix/store/j42vsm6399bgls7f19zjrarnhrmz7p36-gcc-15.3.0/bin:/nix/store/zs6l93qw5hxazyng02srs9jf5r93b8dp-glibc-2.42-67-bin/bin:/nix/store/p79fmimbb698sv4c135kbdwlhjcqpd3p-coreutils-9.11/bin:/nix/store/i2sry2igh6r2vadphmkd9q0ibkkya12n-binutils-wrapper-2.46/bin:/nix/store/4xkx8lf9wn8g4jf923qcvqa51w89gvhg-binutils-2.46/bin:/nix/store/rh2hgxqna9i3xcc5viy8bf4aqcxlxywm-cargo-sweep-0.8.0/bin:/nix/store/nkdvzbfkqln398j6fn4az04z8lmhdnhj-cargo-nextest-0.9.140/bin:/nix/store/s79y60wikybfa2vym2glbz9j7ahi0vfi-cargo-llvm-cov-0.8.7/bin:/nix/store/8cil9jq939jwpzxwh33qp7f1hm0bvl5f-cargo-expand-1.0.124/bin:/nix/store/rbkk6angijxrg446x445200qqw9kkr38-cargo-edit-0.13.13/bin:/nix/store/cgyyhpjr5543k36x2nb1hbh2z68dgg0x-cargo-outdated-0.19.0/bin:/nix/store/3xm2fcrc92b0rbxsf7ll0nr0zbn05arh-cargo-machete-0.9.2/bin:/nix/store/p2lmgwpxz7j5hcn55rx9gsx369zjpj23-cargo-deny-0.20.2/bin:/nix/store/y5mdy3crb5qn43350dlwr9ci8ni40d7k-cargo-hack-0.6.45/bin:/nix/store/wvik6nbz5bkq0cg56ip8b9bdwldqgdxm-cargo-bloat-0.12.1/bin:/nix/store/dar4chx4ggadizhvzq9mpdxhdq3adnrg-cargo-bolero-0.13.4/bin:/nix/store/zqf5c4f0hyi0vdr4f820yfyc6lmvx18g-cargo-watch-8.5.3/bin:/nix/store/64w7zfk9dd7b0q2mc4520z10l8cxq7cy-bacon-3.24.0/bin:/nix/store/1kx6bk7s2q9lgdjff05dnldx2112my3w-cargo-mutants-27.1.0/bin:/nix/store/21cakhvvk291d2r27aqim6djdw0zlkik-bash-interactive-5.3p15/bin:/nix/store/3f5rhrsg2j4x0fhvvh8fir6qf37r56pa-wild-unwrapped-wrapper-0.9.0/bin:/nix/store/c0cr53hv0hcpmjdyc1hrw9fk2i5sx9hn-wild-unwrapped-0.9.0/bin:/nix/store/q376w2vgpkj7xi0lld4fy27vxk04dfp9-clang-wrapper-21.1.8/bin:/nix/store/5wkw09nqmbfsrcksr70lkqzn94x13w0c-clang-21.1.8/bin:/nix/store/rvhsgz04lyh51inkmnslk19ywk9wzvir-binutils-wrapper-2.46/bin:/nix/store/5vviw6b8frj2xs0j34d7dm9rnszwjplf-rust-analyzer-preview-1.100.0-nightly-2026-08-17-aarch64-unknown-linux-gnu/bin:/nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17/bin:/nix/store/nsz7j3qfpidrdbqqppk912jic5isl8n2-clang-tools-21.1.8/bin:/nix/store/wadg45zgfyi9a4dcm5fy3qvlas76cr2z-gnumake-4.4.1/bin:/nix/store/47viyl08bzj7863dvcr8j08wcaiwjaf4-pkg-config-wrapper-0.29.2/bin:/nix/store/n9nrdhqqj8fbdd80q810gfnxs98hipws-ccls-0.20250815.1/bin:/nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2/bin:/nix/store/23j17p6l76nn0d4rxidr2vy488gnvzmb-valgrind-3.27.1/bin:/nix/store/mfbdmmhqzvxh425l7mwk3k5hr2pim2yh-cargo-clippy-fix/bin:/nix/store/f9c5cjwnzxgmny5mp6z01pm3chn22nhy-cargo-fix/bin:/nix/store/jpb6s00z3fhz0nljlwv2zr5577r5j0pa-jujutsu-0.44.0/bin:/nix/store/r6hjn3p91594n85ww9jxs9k230wmwqhh-just-1.57.0/bin:/nix/store/19wxr8hcynk6025kq7yk3vmv6j2yjz21-ripgrep-15.2.0/bin:/nix/store/iahn30z5swk4mdi7pi15cpk8pfzcl7hm-difftastic-0.69.0/bin:/nix/store/xayxls1146z501jzarabbpjq1xy6bpda-prek-0.4.10/bin:/nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped/bin:/nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped/bin:/nix/store/8n1hr4cx9wb6i52jcnrsslz8wiz7r9x7-patchelf-0.15.2/bin:/nix/store/pffmrhqna5mmi75z0213h3s3vvxdqala-compiler-rt-libc-21.1.8/bin:/nix/store/p79fmimbb698sv4c135kbdwlhjcqpd3p-coreutils-9.11/bin:/nix/store/2mj9fbglfqvfizz274hrd8bgvlwv1n0y-findutils-4.11.0/bin:/nix/store/mavadygrg9dx0x4b7rhna9njqlyiplga-diffutils-3.12/bin:/nix/store/jnsbsffmr6zk1y8zic51z4r5ik989gdq-gnused-4.10/bin:/nix/store/cspav3bpfmcsp55lspmc5nvbvyyly1ag-gnugrep-3.12/bin:/nix/store/k03bq9lmv9ssyklfhl852xr8d2isydjd-gawk-5.4.1/bin:/nix/store/8z1grz46krqzzlqbicda7kq383565l8a-gnutar-1.35/bin:/nix/store/wmy40hkad9p9wswn4r8gaqkxsvaby52h-gzip-1.14/bin:/nix/store/phj68a4my78qjjkkqz8mnxkxac7jd5ly-bzip2-1.0.8-bin/bin:/nix/store/mp5ca9knfc0ldra1asdv7xqsm4jq1lmd-gnumake-4.4.1/bin:/nix/store/rzpv2vlq1zmhjqjinjgff1v83nw0b3hz-bash-5.3p15/bin:/nix/store/h3b19sgsmz4vf9ssadhigfnxy9iy1rkr-patch-2.8/bin:/nix/store/i672h9rp9lm8rngn8r6gf8la8z691mkx-xz-5.8.3-bin/bin:/nix/store/y7vsgfs9a32wlhibfmyvwvgvrl737cfv-file-5.48/bin'
export PATH
declare -a unpackCmdHooks=('_defaultUnpack' )
CXX_FOR_BUILD='g++'
export CXX_FOR_BUILD
SOURCE_DATE_EPOCH='315532800'
export SOURCE_DATE_EPOCH
NM_FOR_BUILD='nm'
export NM_FOR_BUILD
DEVENV_STATE='/home/evan/fast-observe/.devenv/state'
export DEVENV_STATE
STRIP='strip'
export STRIP
DEVENV_RUNTIME='/run/user/1000/devenv-707ea10'
export DEVENV_RUNTIME
declare -a envBuildTargetHooks=('ccWrapper_addCVars' 'bintoolsWrapper_addLDVars' )
declare -a postUnpackHooks=('_updateSourceDateEpochFromSourceRoot' )
builder='/nix/store/rzpv2vlq1zmhjqjinjgff1v83nw0b3hz-bash-5.3p15/bin/bash'
export builder
shell='/nix/store/rzpv2vlq1zmhjqjinjgff1v83nw0b3hz-bash-5.3p15/bin/bash'
export shell
RANLIB_FOR_BUILD='ranlib'
export RANLIB_FOR_BUILD
dontAddDisableDepTrack='1'
export dontAddDisableDepTrack
depsBuildBuildPropagated=''
export depsBuildBuildPropagated
preferLocalBuild='1'
export preferLocalBuild
declare -a propagatedTargetDepFiles=('propagated-target-target-deps' )
RANLIB='ranlib'
export RANLIB
LD_FOR_BUILD='ld'
export LD_FOR_BUILD
AR='ar'
export AR
IFS=' 	
'
declare -a postFixupHooks=('noBrokenSymlinksInAllOutputs' '_makeSymlinksRelative' '_multioutPropagateDev' )
NIX_CC_FOR_BUILD='/nix/store/ha244r9jv8a3qyj7m7gr770a00i77vfc-gcc-wrapper-15.3.0'
export NIX_CC_FOR_BUILD
allowSubstitutes=''
export allowSubstitutes
hardeningDisable=''
export hardeningDisable
OBJCOPY='objcopy'
export OBJCOPY
name='devenv-shell-env'
export name
nativeBuildInputs='/nix/store/rh2hgxqna9i3xcc5viy8bf4aqcxlxywm-cargo-sweep-0.8.0 /nix/store/nkdvzbfkqln398j6fn4az04z8lmhdnhj-cargo-nextest-0.9.140 /nix/store/s79y60wikybfa2vym2glbz9j7ahi0vfi-cargo-llvm-cov-0.8.7 /nix/store/8cil9jq939jwpzxwh33qp7f1hm0bvl5f-cargo-expand-1.0.124 /nix/store/rbkk6angijxrg446x445200qqw9kkr38-cargo-edit-0.13.13 /nix/store/cgyyhpjr5543k36x2nb1hbh2z68dgg0x-cargo-outdated-0.19.0 /nix/store/3xm2fcrc92b0rbxsf7ll0nr0zbn05arh-cargo-machete-0.9.2 /nix/store/p2lmgwpxz7j5hcn55rx9gsx369zjpj23-cargo-deny-0.20.2 /nix/store/y5mdy3crb5qn43350dlwr9ci8ni40d7k-cargo-hack-0.6.45 /nix/store/wvik6nbz5bkq0cg56ip8b9bdwldqgdxm-cargo-bloat-0.12.1 /nix/store/dar4chx4ggadizhvzq9mpdxhdq3adnrg-cargo-bolero-0.13.4 /nix/store/zqf5c4f0hyi0vdr4f820yfyc6lmvx18g-cargo-watch-8.5.3 /nix/store/64w7zfk9dd7b0q2mc4520z10l8cxq7cy-bacon-3.24.0 /nix/store/1kx6bk7s2q9lgdjff05dnldx2112my3w-cargo-mutants-27.1.0 /nix/store/8py4d4r6izw1cwpn52z3w2w9whgc859i-bash-interactive-5.3p15-dev /nix/store/3f5rhrsg2j4x0fhvvh8fir6qf37r56pa-wild-unwrapped-wrapper-0.9.0 /nix/store/q376w2vgpkj7xi0lld4fy27vxk04dfp9-clang-wrapper-21.1.8 /nix/store/5vviw6b8frj2xs0j34d7dm9rnszwjplf-rust-analyzer-preview-1.100.0-nightly-2026-08-17-aarch64-unknown-linux-gnu /nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17 /nix/store/nsz7j3qfpidrdbqqppk912jic5isl8n2-clang-tools-21.1.8 /nix/store/5y0xpxiqwpbh2myj7rq97vpacazpgg0g-stdenv-linux /nix/store/wadg45zgfyi9a4dcm5fy3qvlas76cr2z-gnumake-4.4.1 /nix/store/47viyl08bzj7863dvcr8j08wcaiwjaf4-pkg-config-wrapper-0.29.2 /nix/store/n9nrdhqqj8fbdd80q810gfnxs98hipws-ccls-0.20250815.1 /nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2 /nix/store/b2vxsljfjb0n2z735q2y8x0yix1dhzi4-valgrind-3.27.1-dev /nix/store/mfbdmmhqzvxh425l7mwk3k5hr2pim2yh-cargo-clippy-fix /nix/store/f9c5cjwnzxgmny5mp6z01pm3chn22nhy-cargo-fix /nix/store/jpb6s00z3fhz0nljlwv2zr5577r5j0pa-jujutsu-0.44.0 /nix/store/r6hjn3p91594n85ww9jxs9k230wmwqhh-just-1.57.0 /nix/store/19wxr8hcynk6025kq7yk3vmv6j2yjz21-ripgrep-15.2.0 /nix/store/iahn30z5swk4mdi7pi15cpk8pfzcl7hm-difftastic-0.69.0 /nix/store/47viyl08bzj7863dvcr8j08wcaiwjaf4-pkg-config-wrapper-0.29.2 /nix/store/xayxls1146z501jzarabbpjq1xy6bpda-prek-0.4.10 /nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped /nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17 /nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17 /nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped /nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17 /nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17'
export nativeBuildInputs
declare -a propagatedHostDepFiles=('propagated-host-host-deps' 'propagated-build-inputs' )
RUST_SRC_PATH='/nix/store/3j83rmr4ciaml6y93a3wii255cq2xwa3-rust-src-1.100.0-nightly-2026-08-17-aarch64-unknown-linux-gnu/lib/rustlib/src/rust/library'
export RUST_SRC_PATH
doInstallCheck=''
export doInstallCheck
NIX_BINTOOLS_WRAPPER_TARGET_BUILD_aarch64_unknown_linux_gnu='1'
export NIX_BINTOOLS_WRAPPER_TARGET_BUILD_aarch64_unknown_linux_gnu
STRINGS_FOR_BUILD='strings'
export STRINGS_FOR_BUILD
DEVENV_PROFILE='/nix/store/v8s7wsx40bcdp2pc9iwxw9qr2h090h0g-devenv-profile'
export DEVENV_PROFILE
MACHTYPE='aarch64-unknown-linux-gnu'
READELF='readelf'
export READELF
system='aarch64-linux'
export system
declare -a pkgsBuildHost=('/nix/store/rh2hgxqna9i3xcc5viy8bf4aqcxlxywm-cargo-sweep-0.8.0' '/nix/store/nkdvzbfkqln398j6fn4az04z8lmhdnhj-cargo-nextest-0.9.140' '/nix/store/s79y60wikybfa2vym2glbz9j7ahi0vfi-cargo-llvm-cov-0.8.7' '/nix/store/8cil9jq939jwpzxwh33qp7f1hm0bvl5f-cargo-expand-1.0.124' '/nix/store/rbkk6angijxrg446x445200qqw9kkr38-cargo-edit-0.13.13' '/nix/store/cgyyhpjr5543k36x2nb1hbh2z68dgg0x-cargo-outdated-0.19.0' '/nix/store/3xm2fcrc92b0rbxsf7ll0nr0zbn05arh-cargo-machete-0.9.2' '/nix/store/p2lmgwpxz7j5hcn55rx9gsx369zjpj23-cargo-deny-0.20.2' '/nix/store/y5mdy3crb5qn43350dlwr9ci8ni40d7k-cargo-hack-0.6.45' '/nix/store/wvik6nbz5bkq0cg56ip8b9bdwldqgdxm-cargo-bloat-0.12.1' '/nix/store/dar4chx4ggadizhvzq9mpdxhdq3adnrg-cargo-bolero-0.13.4' '/nix/store/zqf5c4f0hyi0vdr4f820yfyc6lmvx18g-cargo-watch-8.5.3' '/nix/store/64w7zfk9dd7b0q2mc4520z10l8cxq7cy-bacon-3.24.0' '/nix/store/1kx6bk7s2q9lgdjff05dnldx2112my3w-cargo-mutants-27.1.0' '/nix/store/8py4d4r6izw1cwpn52z3w2w9whgc859i-bash-interactive-5.3p15-dev' '/nix/store/21cakhvvk291d2r27aqim6djdw0zlkik-bash-interactive-5.3p15' '/nix/store/3f5rhrsg2j4x0fhvvh8fir6qf37r56pa-wild-unwrapped-wrapper-0.9.0' '/nix/store/q376w2vgpkj7xi0lld4fy27vxk04dfp9-clang-wrapper-21.1.8' '/nix/store/rvhsgz04lyh51inkmnslk19ywk9wzvir-binutils-wrapper-2.46' '/nix/store/5vviw6b8frj2xs0j34d7dm9rnszwjplf-rust-analyzer-preview-1.100.0-nightly-2026-08-17-aarch64-unknown-linux-gnu' '/nix/store/4lnlrqhvgjn5p9vawxwv3wxf84dc8qc7-rust-nightly-1.100.0-nightly-2026-08-17-1.100.0-nightly-2026-08-17' '/nix/store/ha244r9jv8a3qyj7m7gr770a00i77vfc-gcc-wrapper-15.3.0' '/nix/store/i2sry2igh6r2vadphmkd9q0ibkkya12n-binutils-wrapper-2.46' '/nix/store/nsz7j3qfpidrdbqqppk912jic5isl8n2-clang-tools-21.1.8' '/nix/store/5y0xpxiqwpbh2myj7rq97vpacazpgg0g-stdenv-linux' '/nix/store/wadg45zgfyi9a4dcm5fy3qvlas76cr2z-gnumake-4.4.1' '/nix/store/47viyl08bzj7863dvcr8j08wcaiwjaf4-pkg-config-wrapper-0.29.2' '/nix/store/n9nrdhqqj8fbdd80q810gfnxs98hipws-ccls-0.20250815.1' '/nix/store/ig4npabmf4vf07k5m4h1x2r13x4s7yfb-gdb-17.2' '/nix/store/b2vxsljfjb0n2z735q2y8x0yix1dhzi4-valgrind-3.27.1-dev' '/nix/store/23j17p6l76nn0d4rxidr2vy488gnvzmb-valgrind-3.27.1' '/nix/store/mfbdmmhqzvxh425l7mwk3k5hr2pim2yh-cargo-clippy-fix' '/nix/store/f9c5cjwnzxgmny5mp6z01pm3chn22nhy-cargo-fix' '/nix/store/jpb6s00z3fhz0nljlwv2zr5577r5j0pa-jujutsu-0.44.0' '/nix/store/r6hjn3p91594n85ww9jxs9k230wmwqhh-just-1.57.0' '/nix/store/19wxr8hcynk6025kq7yk3vmv6j2yjz21-ripgrep-15.2.0' '/nix/store/iahn30z5swk4mdi7pi15cpk8pfzcl7hm-difftastic-0.69.0' '/nix/store/xayxls1146z501jzarabbpjq1xy6bpda-prek-0.4.10' '/nix/store/qpr0vlih07war2i43cw49b50wv8bspgs-clippy-wrapped' '/nix/store/hdhb50iv783ianc3zny7zjglmvdbcwxn-rustfmt-wrapped' '/nix/store/8n1hr4cx9wb6i52jcnrsslz8wiz7r9x7-patchelf-0.15.2' '/nix/store/mxxl2xv4dfn002bqydyv85im2aa2rlgg-update-autotools-gnu-config-scripts-hook' '/nix/store/0y5xmdb7qfvimjwbq7ibg1xdgkgjwqng-no-broken-symlinks.sh' '/nix/store/cv1d7p48379km6a85h4zp6kr86brh32q-audit-tmpdir.sh' '/nix/store/85clx3b0xkdf58jn161iy80y5223ilbi-compress-man-pages.sh' '/nix/store/p3l1a5y7nllfyrjn2krlwgcc3z0cd3fq-make-symlinks-relative.sh' '/nix/store/5yzw0vhkyszf2d179m0qfkgxmp5wjjx4-move-docs.sh' '/nix/store/fyaryjvghbkpfnsyw97hb3lyb37s1pd6-move-lib64.sh' '/nix/store/kd4xwxjpjxi71jkm6ka0np72if9rm3y0-move-sbin.sh' '/nix/store/pag6l61paj1dc9sv15l7bm5c17xn5kyk-move-systemd-user-units.sh' '/nix/store/cmzya9irvxzlkh7lfy6i82gbp0saxqj3-multiple-outputs.sh' '/nix/store/x8c40nfigps493a07sdr2pm5s9j1cdc0-patch-shebangs.sh' '/nix/store/cickvswrvann041nqxb0rxilc46svw1n-prune-libtool-files.sh' '/nix/store/xyff06pkhki3qy1ls77w10s0v79c9il0-reproducible-builds.sh' '/nix/store/z7k98578dfzi6l3hsvbivzm7hfqlk0zc-set-source-date-epoch-to-latest.sh' '/nix/store/pilsssjjdxvdphlg2h19p0bfx5q0jzkn-strip.sh' )
AS_FOR_BUILD='as'
export AS_FOR_BUILD
appendToVar ()
{
 
    local -n nameref="$1";
    local useArray type;
    if [ -n "$__structuredAttrs" ]; then
        useArray=true;
    else
        useArray=false;
    fi;
    if type=$(declare -p "$1" 2> /dev/null); then
        case "${type#* }" in 
            -A*)
                echo "appendToVar(): ERROR: trying to use appendToVar on an associative array, use variable+=([\"X\"]=\"Y\") instead." 1>&2;
                return 1
            ;;
            -a*)
                useArray=true
            ;;
            *)
                useArray=false
            ;;
        esac;
    fi;
    shift;
    if $useArray; then
        nameref=(${nameref+"${nameref[@]}"} "$@");
    else
        nameref="${nameref-} $*";
    fi
}
nixVomitLog ()
{
 
    _nixLogWithLevel 7 "$*"
}
showPhaseHeader ()
{
 
    local phase="$1";
    echo "Running phase: $phase";
    if [[ -z ${NIX_LOG_FD-} ]]; then
        return;
    fi;
    printf "@nix { \"action\": \"setPhase\", \"phase\": \"%s\" }\n" "$phase" >&"$NIX_LOG_FD"
}
stripDirs ()
{
 
    local cmd="$1";
    local ranlibCmd="$2";
    local paths="$3";
    local stripFlags="$4";
    local excludeFlags=();
    local pathsNew=;
    [ -z "$cmd" ] && echo "stripDirs: Strip command is empty" 1>&2 && exit 1;
    [ -z "$ranlibCmd" ] && echo "stripDirs: Ranlib command is empty" 1>&2 && exit 1;
    local pattern;
    if [ -n "${stripExclude:-}" ]; then
        for pattern in "${stripExclude[@]}";
        do
            excludeFlags+=(-a '!' '(' -name "$pattern" -o -wholename "$prefix/$pattern" ')');
        done;
    fi;
    local p;
    for p in ${paths};
    do
        if [ -e "$prefix/$p" ]; then
            pathsNew="${pathsNew} $prefix/$p";
        fi;
    done;
    paths=${pathsNew};
    if [ -n "${paths}" ]; then
        echo "stripping (with command $cmd and flags $stripFlags) in $paths";
        local striperr;
        striperr="$(mktemp --tmpdir="$TMPDIR" 'striperr.XXXXXX')";
        find $paths -type f "${excludeFlags[@]}" -a '!' -path "$prefix/lib/debug/*" -printf '%D-%i,%p\0' | sort -t, -k1,1 -u -z | cut -d, -f2- -z | xargs -r -0 -n1 -P "$NIX_BUILD_CORES" -- $cmd $stripFlags 2> "$striperr" || exit_code=$?;
        [[ "$exit_code" = 123 || -z "$exit_code" ]] || ( cat "$striperr" 1>&2 && exit 1 );
        rm "$striperr";
        find $paths -name '*.a' -type f -exec $ranlibCmd '{}' \; 2> /dev/null;
    fi
}
_multioutDevs ()
{
 
    if [ "$(getAllOutputNames)" = "out" ] || [ -z "${moveToDev-1}" ]; then
        return;
    fi;
    moveToOutput include "${!outputInclude}";
    moveToOutput lib/pkgconfig "${!outputDev}";
    moveToOutput share/pkgconfig "${!outputDev}";
    moveToOutput lib/cmake "${!outputDev}";
    moveToOutput share/aclocal "${!outputDev}";
    for f in "${!outputDev}"/{lib,share}/pkgconfig/*.pc;
    do
        echo "Patching '$f' includedir to output ${!outputInclude}";
        sed -i "/^includedir=/s,=\${prefix},=${!outputInclude}," "$f";
    done
}
recordPropagatedDependencies ()
{
 
    declare -ra flatVars=(depsBuildBuildPropagated propagatedNativeBuildInputs depsBuildTargetPropagated depsHostHostPropagated propagatedBuildInputs depsTargetTargetPropagated);
    declare -ra flatFiles=("${propagatedBuildDepFiles[@]}" "${propagatedHostDepFiles[@]}" "${propagatedTargetDepFiles[@]}");
    local propagatedInputsIndex;
    for propagatedInputsIndex in "${!flatVars[@]}";
    do
        local propagatedInputsSlice="${flatVars[$propagatedInputsIndex]}[@]";
        local propagatedInputsFile="${flatFiles[$propagatedInputsIndex]}";
        [[ -n "${!propagatedInputsSlice}" ]] || continue;
        mkdir -p "${!outputDev}/nix-support";
        printWords ${!propagatedInputsSlice} > "${!outputDev}/nix-support/$propagatedInputsFile";
    done
}
_overrideFirst ()
{
 
    if [ -z "${!1-}" ]; then
        _assignFirst "$@";
    fi
}
patchELF ()
{
 
    local dir="$1";
    [ -e "$dir" ] || return 0;
    echo "shrinking RPATHs of ELF executables and libraries in $dir";
    local i;
    while IFS= read -r -d '' i; do
        if [[ "$i" =~ .build-id ]]; then
            continue;
        fi;
        if ! isELF "$i"; then
            continue;
        fi;
        echo "shrinking $i";
        patchelf --shrink-rpath "$i" || true;
    done < <(find "$dir" -type f -print0)
}
_moveSystemdUserUnits ()
{
 
    if [ "${dontMoveSystemdUserUnits:-0}" = 1 ]; then
        return;
    fi;
    if [ ! -e "${prefix:?}/lib/systemd/user" ]; then
        return;
    fi;
    local source="$prefix/lib/systemd/user";
    local target="$prefix/share/systemd/user";
    echo "moving $source/* to $target";
    mkdir -p "$target";
    ( shopt -s dotglob;
    for i in "$source"/*;
    do
        mv "$i" "$target";
    done );
    rmdir "$source";
    ln -s "$target" "$source"
}
getRole ()
{
 
    case $1 in 
        -1)
            role_post='_FOR_BUILD'
        ;;
        0)
            role_post=''
        ;;
        1)
            role_post='_FOR_TARGET'
        ;;
        *)
            echo "pkg-config-role-hook: used as improper sort of dependency" 1>&2;
            return 1
        ;;
    esac
}
patchPhase ()
{
 
    runHook prePatch;
    local -a patchesArray;
    concatTo patchesArray patches;
    local -a flagsArray;
    concatTo flagsArray patchFlags=-p1;
    for i in "${patchesArray[@]}";
    do
        echo "applying patch $i";
        local uncompress=cat;
        case "$i" in 
            *.gz)
                uncompress="gzip -d"
            ;;
            *.bz2)
                uncompress="bzip2 -d"
            ;;
            *.xz)
                uncompress="xz -d"
            ;;
            *.lzma)
                uncompress="lzma -d"
            ;;
        esac;
        $uncompress < "$i" 2>&1 | patch "${flagsArray[@]}";
    done;
    runHook postPatch
}
substituteAllStream ()
{
 
    local -a args=();
    _allFlags;
    substituteStream "$1" "$2" "${args[@]}"
}
_defaultUnpack ()
{
 
    local fn="$1";
    local destination;
    if [ -d "$fn" ]; then
        destination="$(stripHash "$fn")";
        if [ -e "$destination" ]; then
            echo "Cannot copy $fn to $destination: destination already exists!";
            echo "Did you specify two \"srcs\" with the same \"name\"?";
            return 1;
        fi;
        cp -r --preserve=timestamps --reflink=auto -- "$fn" "$destination";
    else
        case "$fn" in 
            *.tar.xz | *.tar.lzma | *.txz)
                ( XZ_OPT="--threads=$NIX_BUILD_CORES" xz -d < "$fn";
                true ) | tar xf - --mode=+w --warning=no-timestamp
            ;;
            *.tar | *.tar.* | *.tgz | *.tbz2 | *.tbz)
                tar xf "$fn" --mode=+w --warning=no-timestamp
            ;;
            *)
                return 1
            ;;
        esac;
    fi
}
_multioutPropagateDev ()
{
 
    if [ "$(getAllOutputNames)" = "out" ]; then
        return;
    fi;
    local outputFirst;
    for outputFirst in $(getAllOutputNames);
    do
        break;
    done;
    local propagaterOutput="$outputDev";
    if [ -z "$propagaterOutput" ]; then
        propagaterOutput="$outputFirst";
    fi;
    if [ -z "${propagatedBuildOutputs+1}" ]; then
        local po_dirty="$outputBin $outputInclude $outputLib";
        set +o pipefail;
        propagatedBuildOutputs=`echo "$po_dirty"             | tr -s ' ' '\n' | grep -v -F "$propagaterOutput"             | sort -u | tr '\n' ' ' `;
        set -o pipefail;
    fi;
    if [ -z "$propagatedBuildOutputs" ]; then
        return;
    fi;
    mkdir -p "${!propagaterOutput}"/nix-support;
    for output in $propagatedBuildOutputs;
    do
        echo -n " ${!output}" >> "${!propagaterOutput}"/nix-support/propagated-build-inputs;
    done
}
_updateSourceDateEpochFromSourceRoot ()
{
 
    if [ -n "$sourceRoot" ]; then
        updateSourceDateEpoch "$sourceRoot";
    fi
}
fixupPhase ()
{
 
    local output;
    for output in $(getAllOutputNames);
    do
        if [ -e "${!output}" ]; then
            chmod -R u+w,u-s,g-s "${!output}";
        fi;
    done;
    runHook preFixup;
    local output;
    for output in $(getAllOutputNames);
    do
        prefix="${!output}" runHook fixupOutput;
    done;
    recordPropagatedDependencies;
    if [ -n "${setupHook:-}" ]; then
        mkdir -p "${!outputDev}/nix-support";
        substituteAll "$setupHook" "${!outputDev}/nix-support/setup-hook";
    fi;
    if [ -n "${setupHooks:-}" ]; then
        mkdir -p "${!outputDev}/nix-support";
        local hook;
        for hook in ${setupHooks[@]};
        do
            local content;
            consumeEntire content < "$hook";
            substituteAllStream content "file '$hook'" >> "${!outputDev}/nix-support/setup-hook";
            unset -v content;
        done;
        unset -v hook;
    fi;
    if [ -n "${propagatedUserEnvPkgs[*]:-}" ]; then
        mkdir -p "${!outputBin}/nix-support";
        printWords "${propagatedUserEnvPkgs[@]}" > "${!outputBin}/nix-support/propagated-user-env-packages";
    fi;
    runHook postFixup
}
_pruneLibtoolFiles ()
{
 
    if [ "${dontPruneLibtoolFiles-}" ] || [ ! -e "$prefix" ]; then
        return;
    fi;
    find "$prefix" -type f -name '*.la' -exec grep -q '^# Generated by .*libtool' {} \; -exec grep -q "^old_library=''" {} \; -exec sed -i {} -e "/^dependency_libs='[^']/ c dependency_libs='' #pruned" \;
}
concatTo ()
{
 
    local -;
    set -o noglob;
    local -n targetref="$1";
    shift;
    local arg default name type;
    for arg in "$@";
    do
        IFS="=" read -r name default <<< "$arg";
        local -n nameref="$name";
        if [[ -z "${nameref[*]}" && -n "$default" ]]; then
            targetref+=("$default");
        else
            if type=$(declare -p "$name" 2> /dev/null); then
                case "${type#* }" in 
                    -A*)
                        echo "concatTo(): ERROR: trying to use concatTo on an associative array." 1>&2;
                        return 1
                    ;;
                    -a*)
                        targetref+=("${nameref[@]}")
                    ;;
                    *)
                        if [[ "$name" = *"Array" ]]; then
                            nixErrorLog "concatTo(): $name is not declared as array, treating as a singleton. This will become an error in future";
                            targetref+=(${nameref+"${nameref[@]}"});
                        else
                            targetref+=(${nameref-});
                        fi
                    ;;
                esac;
            fi;
        fi;
    done
}
bintoolsWrapper_addLDVars ()
{
 
    local role_post;
    getHostRoleEnvHook;
    if [[ -d "$1/lib64" && ! -L "$1/lib64" ]]; then
        export NIX_LDFLAGS${role_post}+=" -L$1/lib64";
    fi;
    if [[ -d "$1/lib" ]]; then
        local -a glob=($1/lib/lib*);
        if [ "${#glob[*]}" -gt 0 ]; then
            export NIX_LDFLAGS${role_post}+=" -L$1/lib";
        fi;
    fi
}
_callImplicitHook ()
{
 
    local def="$1";
    local hookName="$2";
    if declare -F "$hookName" > /dev/null; then
        nixTalkativeLog "calling implicit '$hookName' function hook";
        "$hookName";
    else
        if type -p "$hookName" > /dev/null; then
            nixTalkativeLog "sourcing implicit '$hookName' script hook";
            source "$hookName";
        else
            if [ -n "${!hookName:-}" ]; then
                nixTalkativeLog "evaling implicit '$hookName' string hook";
                eval "${!hookName}";
            else
                return "$def";
            fi;
        fi;
    fi
}
nixInfoLog ()
{
 
    _nixLogWithLevel 3 "$*"
}
compressManPages ()
{
 
    local dir="$1";
    if [ -L "$dir"/share ] || [ -L "$dir"/share/man ] || [ ! -d "$dir/share/man" ]; then
        return;
    fi;
    echo "gzipping man pages under $dir/share/man/";
    find "$dir"/share/man/ -type f -a '!' -regex '.*\.\(bz2\|gz\|xz\)$' -print0 | xargs -0 -n1 -P "$NIX_BUILD_CORES" gzip -n -f;
    find "$dir"/share/man/ -type l -a '!' -regex '.*\.\(bz2\|gz\|xz\)$' -print0 | sort -z | while IFS= read -r -d '' f; do
        local target;
        target="$(readlink -f "$f")";
        if [ -f "$target".gz ]; then
            ln -sf "$target".gz "$f".gz && rm "$f";
        fi;
    done
}
nixNoticeLog ()
{
 
    _nixLogWithLevel 2 "$*"
}
nixWarnLog ()
{
 
    _nixLogWithLevel 1 "$*"
}
mapOffset ()
{
 
    local -r inputOffset="$1";
    local -n outputOffset="$2";
    if (( inputOffset <= 0 )); then
        outputOffset=$((inputOffset + hostOffset));
    else
        outputOffset=$((inputOffset - 1 + targetOffset));
    fi
}
concatStringsSep ()
{
 
    local sep="$1";
    local name="$2";
    local type oldifs;
    if type=$(declare -p "$name" 2> /dev/null); then
        local -n nameref="$name";
        case "${type#* }" in 
            -A*)
                echo "concatStringsSep(): ERROR: trying to use concatStringsSep on an associative array." 1>&2;
                return 1
            ;;
            -a*)
                local IFS="$(printf '\036')"
            ;;
            *)
                local IFS=" "
            ;;
        esac;
        local ifs_separated="${nameref[*]}";
        echo -n "${ifs_separated//"$IFS"/"$sep"}";
    fi
}
unpackPhase ()
{
 
    runHook preUnpack;
    if [ -z "${srcs:-}" ]; then
        if [ -z "${src:-}" ]; then
            echo 'variable $src or $srcs should point to the source';
            exit 1;
        fi;
        srcs="$src";
    fi;
    local -a srcsArray;
    concatTo srcsArray srcs;
    local dirsBefore="";
    for i in *;
    do
        if [ -d "$i" ]; then
            dirsBefore="$dirsBefore $i ";
        fi;
    done;
    for i in "${srcsArray[@]}";
    do
        unpackFile "$i";
    done;
    : "${sourceRoot=}";
    if [ -n "${setSourceRoot:-}" ]; then
        runOneHook setSourceRoot;
    else
        if [ -z "$sourceRoot" ]; then
            for i in *;
            do
                if [ -d "$i" ]; then
                    case $dirsBefore in 
                        *\ $i\ *)

                        ;;
                        *)
                            if [ -n "$sourceRoot" ]; then
                                echo "unpacker produced multiple directories";
                                exit 1;
                            fi;
                            sourceRoot="$i"
                        ;;
                    esac;
                fi;
            done;
        fi;
    fi;
    if [ -z "$sourceRoot" ]; then
        echo "unpacker appears to have produced no directories";
        exit 1;
    fi;
    echo "source root is $sourceRoot";
    if [ "${dontMakeSourcesWritable:-0}" != 1 ]; then
        chmod -R u+w -- "$sourceRoot";
    fi;
    runHook postUnpack
}
addEnvHooks ()
{
 
    local depHostOffset="$1";
    shift;
    local pkgHookVarsSlice="${pkgHookVarVars[$depHostOffset + 1]}[@]";
    local pkgHookVar;
    for pkgHookVar in "${!pkgHookVarsSlice}";
    do
        eval "${pkgHookVar}s"'+=("$@")';
    done
}
_multioutConfig ()
{
 
    if [ "$(getAllOutputNames)" = "out" ] || [ -z "${setOutputFlags-1}" ]; then
        return;
    fi;
    if [ -z "${shareDocName:-}" ]; then
        local confScript="${configureScript:-}";
        if [ -z "$confScript" ] && [ -x ./configure ]; then
            confScript=./configure;
        fi;
        if [ -f "$confScript" ]; then
            local shareDocName="$(sed -n "s/^PACKAGE_TARNAME='\(.*\)'$/\1/p" < "$confScript")";
        fi;
        if [ -z "$shareDocName" ] || echo "$shareDocName" | grep -q '[^a-zA-Z0-9_-]'; then
            shareDocName="$(echo "$name" | sed 's/-[^a-zA-Z].*//')";
        fi;
    fi;
    prependToVar configureFlags --bindir="${!outputBin}"/bin --sbindir="${!outputBin}"/sbin --includedir="${!outputInclude}"/include --mandir="${!outputMan}"/share/man --infodir="${!outputInfo}"/share/info --docdir="${!outputDoc}"/share/doc/"${shareDocName}" --libdir="${!outputLib}"/lib --libexecdir="${!outputLib}"/libexec --localedir="${!outputLib}"/share/locale;
    prependToVar installFlags pkgconfigdir="${!outputDev}"/lib/pkgconfig m4datadir="${!outputDev}"/share/aclocal aclocaldir="${!outputDev}"/share/aclocal
}
_multioutDocs ()
{
 
    local REMOVE=REMOVE;
    moveToOutput share/info "${!outputInfo}";
    moveToOutput share/doc "${!outputDoc}";
    moveToOutput share/gtk-doc "${!outputDevdoc}";
    moveToOutput share/devhelp/books "${!outputDevdoc}";
    moveToOutput share/man "${!outputMan}";
    moveToOutput share/man/man3 "${!outputDevman}"
}
buildPhase ()
{
 
    runHook preBuild;
    if [[ -z "${makeFlags-}" && -z "${makefile:-}" && ! ( -e Makefile || -e makefile || -e GNUmakefile ) ]]; then
        echo "no Makefile or custom buildPhase, doing nothing";
    else
        foundMakefile=1;
        local flagsArray=(${enableParallelBuilding:+-j${NIX_BUILD_CORES}} SHELL="$SHELL");
        concatTo flagsArray makeFlags makeFlagsArray buildFlags buildFlagsArray;
        echoCmd 'build flags' "${flagsArray[@]}";
        make ${makefile:+-f $makefile} "${flagsArray[@]}";
        unset flagsArray;
    fi;
    runHook postBuild
}
nixLog ()
{
 
    [[ -z ${NIX_LOG_FD-} ]] && return 0;
    local callerName="${FUNCNAME[1]}";
    if [[ $callerName == "_callImplicitHook" ]]; then
        callerName="${hookName:?}";
    fi;
    printf "%s: %s\n" "$callerName" "$*" >&"$NIX_LOG_FD"
}
_assignFirst ()
{
 
    local varName="$1";
    local _var;
    local REMOVE=REMOVE;
    shift;
    for _var in "$@";
    do
        if [ -n "${!_var-}" ]; then
            eval "${varName}"="${_var}";
            return;
        fi;
    done;
    echo;
    echo "error: _assignFirst: could not find a non-empty variable whose name to assign to ${varName}.";
    echo "       The following variables were all unset or empty:";
    echo "           $*";
    if [ -z "${out:-}" ]; then
        echo '       If you do not want an "out" output in your derivation, make sure to define';
        echo '       the other specific required outputs. This can be achieved by picking one';
        echo "       of the above as an output.";
        echo '       You do not have to remove "out" if you want to have a different default';
        echo '       output, because the first output is taken as a default.';
        echo;
    fi;
    return 1
}
printLines ()
{
 
    (( "$#" > 0 )) || return 0;
    printf '%s\n' "$@"
}
distPhase ()
{
 
    runHook preDist;
    local flagsArray=();
    concatTo flagsArray distFlags distFlagsArray distTarget=dist;
    echo 'dist flags: %q' "${flagsArray[@]}";
    make ${makefile:+-f $makefile} "${flagsArray[@]}";
    if [ "${dontCopyDist:-0}" != 1 ]; then
        mkdir -p "$out/tarballs";
        cp -pvd ${tarballs[*]:-*.tar.gz} "$out/tarballs";
    fi;
    runHook postDist
}
_addRpathPrefix ()
{
 
    if [ "${NIX_NO_SELF_RPATH:-0}" != 1 ]; then
        export NIX_LDFLAGS="-rpath $1/lib ${NIX_LDFLAGS-}";
    fi
}
installCheckPhase ()
{
 
    runHook preInstallCheck;
    if [[ -z "${foundMakefile:-}" ]]; then
        echo "no Makefile or custom installCheckPhase, doing nothing";
    else
        if [[ -z "${installCheckTarget:-}" ]] && ! make -n ${makefile:+-f $makefile} "${installCheckTarget:-installcheck}" > /dev/null 2>&1; then
            echo "no installcheck target in ${makefile:-Makefile}, doing nothing";
        else
            local flagsArray=(${enableParallelChecking:+-j${NIX_BUILD_CORES}} SHELL="$SHELL");
            concatTo flagsArray makeFlags makeFlagsArray installCheckFlags installCheckFlagsArray installCheckTarget=installcheck;
            echoCmd 'installcheck flags' "${flagsArray[@]}";
            make ${makefile:+-f $makefile} "${flagsArray[@]}";
            unset flagsArray;
        fi;
    fi;
    runHook postInstallCheck
}
findInputs ()
{
 
    local -r pkg="$1";
    local -r hostOffset="$2";
    local -r targetOffset="$3";
    (( hostOffset <= targetOffset )) || exit 1;
    local varVar="${pkgAccumVarVars[hostOffset + 1]}";
    local varRef="$varVar[$((targetOffset - hostOffset))]";
    local var="${!varRef}";
    unset -v varVar varRef;
    local varSlice="$var[*]";
    case " ${!varSlice-} " in 
        *" $pkg "*)
            return 0
        ;;
    esac;
    unset -v varSlice;
    eval "$var"'+=("$pkg")';
    if ! [ -e "$pkg" ]; then
        echo "build input $pkg does not exist" 1>&2;
        exit 1;
    fi;
    function mapOffset () 
    { 
        local -r inputOffset="$1";
        local -n outputOffset="$2";
        if (( inputOffset <= 0 )); then
            outputOffset=$((inputOffset + hostOffset));
        else
            outputOffset=$((inputOffset - 1 + targetOffset));
        fi
    };
    local relHostOffset;
    for relHostOffset in "${allPlatOffsets[@]}";
    do
        local files="${propagatedDepFilesVars[relHostOffset + 1]}";
        local hostOffsetNext;
        mapOffset "$relHostOffset" hostOffsetNext;
        (( -1 <= hostOffsetNext && hostOffsetNext <= 1 )) || continue;
        local relTargetOffset;
        for relTargetOffset in "${allPlatOffsets[@]}";
        do
            (( "$relHostOffset" <= "$relTargetOffset" )) || continue;
            local fileRef="${files}[$relTargetOffset - $relHostOffset]";
            local file="${!fileRef}";
            unset -v fileRef;
            local targetOffsetNext;
            mapOffset "$relTargetOffset" targetOffsetNext;
            (( -1 <= hostOffsetNext && hostOffsetNext <= 1 )) || continue;
            [[ -f "$pkg/nix-support/$file" ]] || continue;
            local pkgNext;
            read -r -d '' pkgNext < "$pkg/nix-support/$file" || true;
            for pkgNext in $pkgNext;
            do
                findInputs "$pkgNext" "$hostOffsetNext" "$targetOffsetNext";
            done;
        done;
    done
}
getTargetRole ()
{
 
    getRole "$targetOffset"
}
nixErrorLog ()
{
 
    _nixLogWithLevel 0 "$*"
}
stripHash ()
{
 
    local strippedName casematchOpt=0;
    strippedName="$(basename -- "$1")";
    shopt -q nocasematch && casematchOpt=1;
    shopt -u nocasematch;
    if [[ "$strippedName" =~ ^[a-z0-9]{32}- ]]; then
        echo "${strippedName:33}";
    else
        echo "$strippedName";
    fi;
    if (( casematchOpt )); then
        shopt -s nocasematch;
    fi
}
_moveLib64 ()
{
 
    if [ "${dontMoveLib64-}" = 1 ]; then
        return;
    fi;
    if [ ! -e "$prefix/lib64" -o -L "$prefix/lib64" ]; then
        return;
    fi;
    echo "moving $prefix/lib64/* to $prefix/lib";
    mkdir -p $prefix/lib;
    shopt -s dotglob;
    for i in $prefix/lib64/*;
    do
        mv --no-clobber "$i" $prefix/lib;
    done;
    shopt -u dotglob;
    rmdir $prefix/lib64;
    ln -s lib $prefix/lib64
}
consumeEntire ()
{
 
    if IFS='' read -r -d '' "$1"; then
        echo "consumeEntire(): ERROR: Input null bytes, won't process" 1>&2;
        return 1;
    fi
}
noBrokenSymlinks ()
{
 
    local -r output="${1:?}";
    local path;
    local pathParent;
    local symlinkTarget;
    local -i numDanglingSymlinks=0;
    local -i numReflexiveSymlinks=0;
    local -i numUnreadableSymlinks=0;
    if [[ ! -e $output ]]; then
        nixWarnLog "skipping non-existent output $output";
        return 0;
    fi;
    nixInfoLog "running on $output";
    while IFS= read -r -d '' path; do
        pathParent="$(dirname "$path")";
        if ! symlinkTarget="$(readlink "$path")"; then
            nixErrorLog "the symlink $path is unreadable";
            numUnreadableSymlinks+=1;
            continue;
        fi;
        if [[ $symlinkTarget == /* ]]; then
            nixInfoLog "symlink $path points to absolute target $symlinkTarget";
        else
            nixInfoLog "symlink $path points to relative target $symlinkTarget";
            symlinkTarget="$(realpath --no-symlinks --canonicalize-missing "$pathParent/$symlinkTarget")";
        fi;
        if [[ $symlinkTarget = "$TMPDIR"/* ]]; then
            nixErrorLog "the symlink $path points to $TMPDIR directory: $symlinkTarget";
            numDanglingSymlinks+=1;
            continue;
        fi;
        if [[ $symlinkTarget != "$NIX_STORE"/* ]]; then
            nixInfoLog "symlink $path points outside the Nix store; ignoring";
            continue;
        fi;
        if [[ $path == "$symlinkTarget" ]]; then
            nixErrorLog "the symlink $path is reflexive";
            numReflexiveSymlinks+=1;
        else
            if [[ ! -e $symlinkTarget ]]; then
                nixErrorLog "the symlink $path points to a missing target: $symlinkTarget";
                numDanglingSymlinks+=1;
            else
                nixDebugLog "the symlink $path is irreflexive and points to a target which exists";
            fi;
        fi;
    done < <(find "$output" -type l -print0);
    if ((numDanglingSymlinks > 0 || numReflexiveSymlinks > 0 || numUnreadableSymlinks > 0)); then
        nixErrorLog "found $numDanglingSymlinks dangling symlinks, $numReflexiveSymlinks reflexive symlinks and $numUnreadableSymlinks unreadable symlinks";
        exit 1;
    fi;
    return 0
}
dumpVars ()
{
 
    if [[ "${noDumpEnvVars:-0}" != 1 && -d "$NIX_BUILD_TOP" ]]; then
        local old_umask;
        old_umask=$(umask);
        umask 0077;
        export 2> /dev/null > "$NIX_BUILD_TOP/env-vars";
        umask "$old_umask";
    fi
}
getTargetRoleEnvHook ()
{
 
    getRole "$depTargetOffset"
}
fixLibtool ()
{
 
    local search_path;
    for flag in $NIX_LDFLAGS;
    do
        case $flag in 
            -L*)
                search_path+=" ${flag#-L}"
            ;;
        esac;
    done;
    sed -i "$1" -e "s^eval \(sys_lib_search_path=\).*^\1'${search_path:-}'^" -e 's^eval sys_lib_.+search_path=.*^^'
}
_activatePkgs ()
{
 
    local hostOffset targetOffset;
    local pkg;
    for hostOffset in "${allPlatOffsets[@]}";
    do
        local pkgsVar="${pkgAccumVarVars[hostOffset + 1]}";
        for targetOffset in "${allPlatOffsets[@]}";
        do
            (( hostOffset <= targetOffset )) || continue;
            local pkgsRef="${pkgsVar}[$targetOffset - $hostOffset]";
            local pkgsSlice="${!pkgsRef}[@]";
            for pkg in ${!pkgsSlice+"${!pkgsSlice}"};
            do
                activatePackage "$pkg" "$hostOffset" "$targetOffset";
            done;
        done;
    done
}
genericBuild ()
{
 
    export GZIP_NO_TIMESTAMPS=1;
    if [ -f "${buildCommandPath:-}" ]; then
        source "$buildCommandPath";
        return;
    fi;
    if [ -n "${buildCommand:-}" ]; then
        eval "$buildCommand";
        return;
    fi;
    definePhases;
    for curPhase in ${phases[*]};
    do
        runPhase "$curPhase";
    done
}
nixTalkativeLog ()
{
 
    _nixLogWithLevel 4 "$*"
}
pkgConfigWrapper_addPkgConfigPath ()
{
 
    local role_post;
    getHostRoleEnvHook;
    addToSearchPath "PKG_CONFIG_PATH${role_post}" "$1/lib/pkgconfig";
    addToSearchPath "PKG_CONFIG_PATH${role_post}" "$1/share/pkgconfig"
}
printPhases ()
{
 
    definePhases;
    local phase;
    for phase in ${phases[*]};
    do
        printf '%s\n' "$phase";
    done
}
_logHook ()
{
 
    if [[ -z ${NIX_LOG_FD-} ]]; then
        return;
    fi;
    local hookKind="$1";
    local hookExpr="$2";
    shift 2;
    if declare -F "$hookExpr" > /dev/null 2>&1; then
        nixTalkativeLog "calling '$hookKind' function hook '$hookExpr'" "$@";
    else
        if type -p "$hookExpr" > /dev/null; then
            nixTalkativeLog "sourcing '$hookKind' script hook '$hookExpr'";
        else
            if [[ "$hookExpr" != "_callImplicitHook"* ]]; then
                local exprToOutput;
                if [[ ${NIX_DEBUG:-0} -ge 5 ]]; then
                    exprToOutput="$hookExpr";
                else
                    local hookExprLine;
                    while IFS= read -r hookExprLine; do
                        hookExprLine="${hookExprLine#"${hookExprLine%%[![:space:]]*}"}";
                        if [[ -n "$hookExprLine" ]]; then
                            exprToOutput+="$hookExprLine\\n ";
                        fi;
                    done <<< "$hookExpr";
                    exprToOutput="${exprToOutput%%\\n }";
                fi;
                nixTalkativeLog "evaling '$hookKind' string hook '$exprToOutput'";
            fi;
        fi;
    fi
}
isMachO ()
{
 
    local fn="$1";
    local fd;
    local magic;
    exec {fd}< "$fn";
    LANG=C read -r -n 4 -u "$fd" magic;
    exec {fd}>&-;
    if [[ "$magic" = $(echo -ne "\xfe\xed\xfa\xcf") || "$magic" = $(echo -ne "\xcf\xfa\xed\xfe") ]]; then
        return 0;
    else
        if [[ "$magic" = $(echo -ne "\xfe\xed\xfa\xce") || "$magic" = $(echo -ne "\xce\xfa\xed\xfe") ]]; then
            return 0;
        else
            if [[ "$magic" = $(echo -ne "\xca\xfe\xba\xbe") || "$magic" = $(echo -ne "\xbe\xba\xfe\xca") ]]; then
                return 0;
            else
                return 1;
            fi;
        fi;
    fi
}
runHook ()
{
 
    local hookName="$1";
    shift;
    local hooksSlice="${hookName%Hook}Hooks[@]";
    local hook;
    for hook in "_callImplicitHook 0 $hookName" ${!hooksSlice+"${!hooksSlice}"};
    do
        _logHook "$hookName" "$hook" "$@";
        _eval "$hook" "$@";
    done;
    return 0
}
getHostRoleEnvHook ()
{
 
    getRole "$depHostOffset"
}
printWords ()
{
 
    (( "$#" > 0 )) || return 0;
    printf '%s ' "$@"
}
exitHandler ()
{
 
    exitCode="$?";
    set +e;
    if [ -n "${showBuildStats:-}" ]; then
        read -r -d '' -a buildTimes < <(times);
        echo "build times:";
        echo "user time for the shell             ${buildTimes[0]}";
        echo "system time for the shell           ${buildTimes[1]}";
        echo "user time for all child processes   ${buildTimes[2]}";
        echo "system time for all child processes ${buildTimes[3]}";
    fi;
    if (( "$exitCode" != 0 )); then
        runHook failureHook;
        if [ -n "${succeedOnFailure:-}" ]; then
            echo "build failed with exit code $exitCode (ignored)";
            mkdir -p "$out/nix-support";
            printf "%s" "$exitCode" > "$out/nix-support/failed";
            exit 0;
        fi;
    else
        runHook exitHook;
    fi;
    return "$exitCode"
}
getHostRole ()
{
 
    getRole "$hostOffset"
}
nixDebugLog ()
{
 
    _nixLogWithLevel 6 "$*"
}
_eval ()
{
 
    if declare -F "$1" > /dev/null 2>&1; then
        "$@";
    else
        eval "$1";
    fi
}
patchShebangs ()
{
 
    local pathName;
    local update=false;
    while [[ $# -gt 0 ]]; do
        case "$1" in 
            --host)
                pathName=HOST_PATH;
                shift
            ;;
            --build)
                pathName=PATH;
                shift
            ;;
            --update)
                update=true;
                shift
            ;;
            --)
                shift;
                break
            ;;
            -* | --*)
                echo "Unknown option $1 supplied to patchShebangs" 1>&2;
                return 1
            ;;
            *)
                break
            ;;
        esac;
    done;
    echo "patching script interpreter paths in $@";
    local f;
    local oldPath;
    local newPath;
    local arg0;
    local args;
    local oldInterpreterLine;
    local newInterpreterLine;
    if [[ $# -eq 0 ]]; then
        echo "No arguments supplied to patchShebangs" 1>&2;
        return 0;
    fi;
    local f;
    while IFS= read -r -d '' f; do
        isScript "$f" || continue;
        read -r oldInterpreterLine < "$f" || [ "$oldInterpreterLine" ];
        read -r oldPath arg0 args <<< "${oldInterpreterLine:2}";
        if [[ -z "${pathName:-}" ]]; then
            if [[ -n $strictDeps && $f == "$NIX_STORE"* ]]; then
                pathName=HOST_PATH;
            else
                pathName=PATH;
            fi;
        fi;
        if [[ "$oldPath" == *"/bin/env" ]]; then
            if [[ $arg0 == "-S" ]]; then
                arg0=${args%% *};
                [[ "$args" == *" "* ]] && args=${args#* } || args=;
                newPath="$(PATH="${!pathName}" type -P "env" || true)";
                args="-S $(PATH="${!pathName}" type -P "$arg0" || true) $args";
            else
                if [[ $arg0 == "-"* || $arg0 == *"="* ]]; then
                    echo "$f: unsupported interpreter directive \"$oldInterpreterLine\" (set dontPatchShebangs=1 and handle shebang patching yourself)" 1>&2;
                    exit 1;
                else
                    newPath="$(PATH="${!pathName}" type -P "$arg0" || true)";
                fi;
            fi;
        else
            if [[ -z $oldPath ]]; then
                oldPath="/bin/sh";
            fi;
            newPath="$(PATH="${!pathName}" type -P "$(basename "$oldPath")" || true)";
            args="$arg0 $args";
        fi;
        newInterpreterLine="$newPath $args";
        newInterpreterLine=${newInterpreterLine%${newInterpreterLine##*[![:space:]]}};
        if [[ -n "$oldPath" && ( "$update" == true || "${oldPath:0:${#NIX_STORE}}" != "$NIX_STORE" ) ]]; then
            if [[ -n "$newPath" && "$newPath" != "$oldPath" ]]; then
                echo "$f: interpreter directive changed from \"$oldInterpreterLine\" to \"$newInterpreterLine\"";
                escapedInterpreterLine=${newInterpreterLine//\\/\\\\};
                timestamp=$(stat --printf "%y" "$f");
                tmpFile=$(mktemp -t patchShebangs.XXXXXXXXXX);
                sed -e "1 s|.*|#\!$escapedInterpreterLine|" "$f" > "$tmpFile";
                local restoreReadOnly;
                if [[ ! -w "$f" ]]; then
                    chmod +w "$f";
                    restoreReadOnly=true;
                fi;
                cat "$tmpFile" > "$f";
                rm "$tmpFile";
                if [[ -n "${restoreReadOnly:-}" ]]; then
                    chmod -w "$f";
                fi;
                touch --date "$timestamp" "$f";
            fi;
        fi;
    done < <(find "$@" -type f -perm -0100 -print0)
}
_moveToShare ()
{
 
    if [ -n "$__structuredAttrs" ]; then
        if [ -z "${forceShare-}" ]; then
            forceShare=(man doc info);
        fi;
    else
        forceShare=(${forceShare:-man doc info});
    fi;
    if [[ -z "$out" ]]; then
        return;
    fi;
    for d in "${forceShare[@]}";
    do
        if [ -d "$out/$d" ]; then
            if [ -d "$out/share/$d" ]; then
                echo "both $d/ and share/$d/ exist!";
            else
                echo "moving $out/$d to $out/share/$d";
                mkdir -p $out/share;
                mv $out/$d $out/share/;
            fi;
        fi;
    done
}
_doStrip ()
{
 
    local -ra flags=(dontStripHost dontStripTarget);
    local -ra debugDirs=(stripDebugList stripDebugListTarget);
    local -ra allDirs=(stripAllList stripAllListTarget);
    local -ra stripCmds=(STRIP STRIP_FOR_TARGET);
    local -ra ranlibCmds=(RANLIB RANLIB_FOR_TARGET);
    stripDebugList=${stripDebugList[*]:-lib lib32 lib64 libexec bin sbin Applications Library/Frameworks};
    stripDebugListTarget=${stripDebugListTarget[*]:-};
    stripAllList=${stripAllList[*]:-};
    stripAllListTarget=${stripAllListTarget[*]:-};
    local i;
    for i in ${!stripCmds[@]};
    do
        local -n flag="${flags[$i]}";
        local -n debugDirList="${debugDirs[$i]}";
        local -n allDirList="${allDirs[$i]}";
        local -n stripCmd="${stripCmds[$i]}";
        local -n ranlibCmd="${ranlibCmds[$i]}";
        if [[ -n "${dontStrip-}" || -n "${flag-}" ]] || ! type -f "${stripCmd-}" 2> /dev/null 1>&2; then
            continue;
        fi;
        stripDirs "$stripCmd" "$ranlibCmd" "$debugDirList" "${stripDebugFlags[*]:--S -p}";
        stripDirs "$stripCmd" "$ranlibCmd" "$allDirList" "${stripAllFlags[*]:--s -p}";
    done
}
installPhase ()
{
 
    runHook preInstall;
    if [[ -z "${makeFlags-}" && -z "${makefile:-}" && ! ( -e Makefile || -e makefile || -e GNUmakefile ) ]]; then
        echo "no Makefile or custom installPhase, doing nothing";
        runHook postInstall;
        return;
    else
        foundMakefile=1;
    fi;
    if [ -n "$prefix" ]; then
        mkdir -p "$prefix";
    fi;
    local flagsArray=(${enableParallelInstalling:+-j${NIX_BUILD_CORES}} SHELL="$SHELL");
    concatTo flagsArray makeFlags makeFlagsArray installFlags installFlagsArray installTargets=install;
    echoCmd 'install flags' "${flagsArray[@]}";
    make ${makefile:+-f $makefile} "${flagsArray[@]}";
    unset flagsArray;
    runHook postInstall
}
substitute ()
{
 
    local input="$1";
    local output="$2";
    shift 2;
    if [ ! -f "$input" ]; then
        echo "substitute(): ERROR: file '$input' does not exist" 1>&2;
        return 1;
    fi;
    local content;
    consumeEntire content < "$input";
    if [ -e "$output" ]; then
        chmod +w "$output";
    fi;
    substituteStream content "file '$input'" "$@" > "$output"
}
patchShebangsAuto ()
{
 
    if [[ -z "${dontPatchShebangs-}" && -e "$prefix" ]]; then
        if [[ "$output" != out && "$output" = "$outputDev" ]]; then
            patchShebangs --build "$prefix";
        else
            patchShebangs --host "$prefix";
        fi;
    fi
}
updateAutotoolsGnuConfigScriptsPhase ()
{
 
    if [ -n "${dontUpdateAutotoolsGnuConfigScripts-}" ]; then
        return;
    fi;
    for script in config.sub config.guess;
    do
        for f in $(find . -type f -name "$script");
        do
            echo "Updating Autotools / GNU config script to a newer upstream version: $f";
            cp -f "/nix/store/217bhv89ijjvy0yzm6d9vqhmyl2084vx-gnu-config-2024-01-01/$script" "$f";
        done;
    done
}
justInstallPhase ()
{
 
    runHook preInstall;
    local flagsArray=();
    concatTo flagsArray justFlags justFlagsArray installTargets=install;
    echoCmd 'install flags' "${flagsArray[@]}";
    just "${flagsArray[@]}";
    runHook postInstall
}
_addToEnv ()
{
 
    local depHostOffset depTargetOffset;
    local pkg;
    for depHostOffset in "${allPlatOffsets[@]}";
    do
        local hookVar="${pkgHookVarVars[depHostOffset + 1]}";
        local pkgsVar="${pkgAccumVarVars[depHostOffset + 1]}";
        for depTargetOffset in "${allPlatOffsets[@]}";
        do
            (( depHostOffset <= depTargetOffset )) || continue;
            local hookRef="${hookVar}[$depTargetOffset - $depHostOffset]";
            if [[ -z "${strictDeps-}" ]]; then
                local visitedPkgs="";
                for pkg in "${pkgsBuildBuild[@]}" "${pkgsBuildHost[@]}" "${pkgsBuildTarget[@]}" "${pkgsHostHost[@]}" "${pkgsHostTarget[@]}" "${pkgsTargetTarget[@]}";
                do
                    if [[ "$visitedPkgs" = *"$pkg"* ]]; then
                        continue;
                    fi;
                    runHook "${!hookRef}" "$pkg";
                    visitedPkgs+=" $pkg";
                done;
            else
                local pkgsRef="${pkgsVar}[$depTargetOffset - $depHostOffset]";
                local pkgsSlice="${!pkgsRef}[@]";
                for pkg in ${!pkgsSlice+"${!pkgsSlice}"};
                do
                    runHook "${!hookRef}" "$pkg";
                done;
            fi;
        done;
    done
}
showPhaseFooter ()
{
 
    local phase="$1";
    local startTime="$2";
    local endTime="$3";
    local delta=$(( endTime - startTime ));
    (( delta < 30 )) && return;
    local H=$((delta/3600));
    local M=$((delta%3600/60));
    local S=$((delta%60));
    echo -n "$phase completed in ";
    (( H > 0 )) && echo -n "$H hours ";
    (( M > 0 )) && echo -n "$M minutes ";
    echo "$S seconds"
}
getAllOutputNames ()
{
 
    if [ -n "$__structuredAttrs" ]; then
        echo "${!outputs[*]}";
    else
        echo "$outputs";
    fi
}
_makeSymlinksRelative ()
{
 
    local prefixes;
    prefixes=();
    for output in $(getAllOutputNames);
    do
        [ ! -e "${!output}" ] && continue;
        prefixes+=("${!output}");
    done;
    find "${prefixes[@]}" -type l -printf '%H\0%p\0' | xargs -0 -n2 -r -P "$NIX_BUILD_CORES" sh -c '
      output="$1"
      link="$2"

      linkTarget=$(readlink "$link")

      # only touch links that point inside the same output tree
      [[ $linkTarget == "$output"/* ]] || exit 0

      if [ ! -e "$linkTarget" ]; then
        echo "the symlink $link is broken, it points to $linkTarget (which is missing)"
      fi

      echo "making symlink relative: $link"
      ln -snrf "$linkTarget" "$link"
    ' _
}
substituteAll ()
{
 
    local input="$1";
    local output="$2";
    local -a args=();
    _allFlags;
    substitute "$input" "$output" "${args[@]}"
}
substituteAllInPlace ()
{
 
    local fileName="$1";
    shift;
    substituteAll "$fileName" "$fileName" "$@"
}
justBuildPhase ()
{
 
    runHook preBuild;
    local flagsArray=();
    concatTo flagsArray justFlags justFlagsArray;
    echoCmd 'build flags' "${flagsArray[@]}";
    just "${flagsArray[@]}";
    runHook postBuild
}
justCheckPhase ()
{
 
    runHook preCheck;
    if [ -z "${checkTarget:-}" ]; then
        if just -n test > /dev/null 2>&1; then
            checkTarget="test";
        fi;
    fi;
    if [ -z "${checkTarget:-}" ]; then
        echo "no test target found in just, doing nothing";
    else
        local flagsArray=();
        concatTo flagsArray justFlags justFlagsArray checkTarget;
        echoCmd 'check flags' "${flagsArray[@]}";
        just "${flagsArray[@]}";
    fi;
    runHook postCheck
}
unpackFile ()
{
 
    curSrc="$1";
    echo "unpacking source archive $curSrc";
    if ! runOneHook unpackCmd "$curSrc"; then
        echo "do not know how to unpack source archive $curSrc";
        exit 1;
    fi
}
getTargetRoleWrapper ()
{
 
    case $targetOffset in 
        -1)
            export NIX_PKG_CONFIG_WRAPPER_TARGET_BUILD_aarch64_unknown_linux_gnu=1
        ;;
        0)
            export NIX_PKG_CONFIG_WRAPPER_TARGET_HOST_aarch64_unknown_linux_gnu=1
        ;;
        1)
            export NIX_PKG_CONFIG_WRAPPER_TARGET_TARGET_aarch64_unknown_linux_gnu=1
        ;;
        *)
            echo "pkg-config-role-hook: used as improper sort of dependency" 1>&2;
            return 1
        ;;
    esac
}
_nixLogWithLevel ()
{
 
    [[ -z ${NIX_LOG_FD-} || ${NIX_DEBUG:-0} -lt ${1:?} ]] && return 0;
    local logLevel;
    case "${1:?}" in 
        0)
            logLevel=ERROR
        ;;
        1)
            logLevel=WARN
        ;;
        2)
            logLevel=NOTICE
        ;;
        3)
            logLevel=INFO
        ;;
        4)
            logLevel=TALKATIVE
        ;;
        5)
            logLevel=CHATTY
        ;;
        6)
            logLevel=DEBUG
        ;;
        7)
            logLevel=VOMIT
        ;;
        *)
            echo "_nixLogWithLevel: called with invalid log level: ${1:?}" >&"$NIX_LOG_FD";
            return 1
        ;;
    esac;
    local callerName="${FUNCNAME[2]}";
    if [[ $callerName == "_callImplicitHook" ]]; then
        callerName="${hookName:?}";
    fi;
    printf "%s: %s: %s\n" "$logLevel" "$callerName" "${2:?}" >&"$NIX_LOG_FD"
}
addToSearchPathWithCustomDelimiter ()
{
 
    local delimiter="$1";
    local varName="$2";
    local dir="$3";
    if [[ -d "$dir" && "${!varName:+${delimiter}${!varName}${delimiter}}" != *"${delimiter}${dir}${delimiter}"* ]]; then
        export "${varName}=${!varName:+${!varName}${delimiter}}${dir}";
    fi
}
_allFlags ()
{
 
    export system pname name version;
    while IFS='' read -r varName; do
        nixTalkativeLog "@${varName}@ -> ${!varName}";
        args+=("--subst-var" "$varName");
    done < <(awk 'BEGIN { for (v in ENVIRON) if (v ~ /^[a-z][a-zA-Z0-9_]*$/) print v }')
}
ccWrapper_addCVars ()
{
 
    local role_post;
    getHostRoleEnvHook;
    local found=;
    if [ -d "$1/include" ]; then
        export NIX_CFLAGS_COMPILE${role_post}+=" -isystem $1/include";
        found=1;
    fi;
    if [ -d "$1/Library/Frameworks" ]; then
        export NIX_CFLAGS_COMPILE${role_post}+=" -iframework $1/Library/Frameworks";
        found=1;
    fi;
    if [[ -n "" && -n ${NIX_STORE:-} && -n $found ]]; then
        local scrubbed="$NIX_STORE/eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-${1#"$NIX_STORE"/*-}";
        export NIX_CFLAGS_COMPILE${role_post}+=" -fmacro-prefix-map=$1=$scrubbed";
    fi
}
checkPhase ()
{
 
    runHook preCheck;
    if [[ -z "${foundMakefile:-}" ]]; then
        echo "no Makefile or custom checkPhase, doing nothing";
        runHook postCheck;
        return;
    fi;
    if [[ -z "${checkTarget:-}" ]]; then
        if make -n ${makefile:+-f $makefile} check > /dev/null 2>&1; then
            checkTarget="check";
        else
            if make -n ${makefile:+-f $makefile} test > /dev/null 2>&1; then
                checkTarget="test";
            fi;
        fi;
    fi;
    if [[ -z "${checkTarget:-}" ]]; then
        echo "no check/test target in ${makefile:-Makefile}, doing nothing";
    else
        local flagsArray=(${enableParallelChecking:+-j${NIX_BUILD_CORES}} SHELL="$SHELL");
        concatTo flagsArray makeFlags makeFlagsArray checkFlags=VERBOSE=y checkFlagsArray checkTarget;
        echoCmd 'check flags' "${flagsArray[@]}";
        make ${makefile:+-f $makefile} "${flagsArray[@]}";
        unset flagsArray;
    fi;
    runHook postCheck
}
moveToOutput ()
{
 
    local patt="$1";
    local dstOut="$2";
    local output;
    for output in $(getAllOutputNames);
    do
        if [ "${!output}" = "$dstOut" ]; then
            continue;
        fi;
        local srcPath;
        for srcPath in "${!output}"/$patt;
        do
            if [ ! -e "$srcPath" ] && [ ! -L "$srcPath" ]; then
                continue;
            fi;
            if [ "$dstOut" = REMOVE ]; then
                echo "Removing $srcPath";
                rm -r "$srcPath";
            else
                local dstPath="$dstOut${srcPath#${!output}}";
                echo "Moving $srcPath to $dstPath";
                if [ -d "$dstPath" ] && [ -d "$srcPath" ]; then
                    rmdir "$srcPath" --ignore-fail-on-non-empty;
                    if [ -d "$srcPath" ]; then
                        mv -t "$dstPath" "$srcPath"/*;
                        rmdir "$srcPath";
                    fi;
                else
                    mkdir -p "$(readlink -m "$dstPath/..")";
                    mv "$srcPath" "$dstPath";
                fi;
            fi;
            local srcParent="$(readlink -m "$srcPath/..")";
            if [ -n "$(find "$srcParent" -maxdepth 0 -type d -empty 2> /dev/null)" ]; then
                echo "Removing empty $srcParent/ and (possibly) its parents";
                rmdir -p --ignore-fail-on-non-empty "$srcParent" 2> /dev/null || true;
            fi;
        done;
    done
}
configurePhase ()
{
 
    runHook preConfigure;
    : "${configureScript=}";
    if [[ -z "$configureScript" && -x ./configure ]]; then
        configureScript=./configure;
    fi;
    if [ -z "${dontFixLibtool:-}" ]; then
        export lt_cv_deplibs_check_method="${lt_cv_deplibs_check_method-pass_all}";
        local i;
        find . -iname "ltmain.sh" -print0 | while IFS='' read -r -d '' i; do
            echo "fixing libtool script $i";
            fixLibtool "$i";
        done;
        CONFIGURE_MTIME_REFERENCE=$(mktemp configure.mtime.reference.XXXXXX);
        find . -executable -type f -name configure -exec grep -l 'GNU Libtool is free software; you can redistribute it and/or modify' {} \; -exec touch -r {} "$CONFIGURE_MTIME_REFERENCE" \; -exec sed -i s_/usr/bin/file_file_g {} \; -exec touch -r "$CONFIGURE_MTIME_REFERENCE" {} \;;
        rm -f "$CONFIGURE_MTIME_REFERENCE";
    fi;
    if [[ -z "${dontAddPrefix:-}" && -n "$prefix" ]]; then
        local -r prefixKeyOrDefault="${prefixKey:---prefix=}";
        if [ "${prefixKeyOrDefault: -1}" = " " ]; then
            prependToVar configureFlags "$prefix";
            prependToVar configureFlags "${prefixKeyOrDefault::-1}";
        else
            prependToVar configureFlags "$prefixKeyOrDefault$prefix";
        fi;
    fi;
    if [[ -f "$configureScript" ]]; then
        if [ -z "${dontAddDisableDepTrack:-}" ]; then
            if grep -q dependency-tracking "$configureScript"; then
                prependToVar configureFlags --disable-dependency-tracking;
            fi;
        fi;
        if [ -z "${dontDisableStatic:-}" ]; then
            if grep -q enable-static "$configureScript"; then
                prependToVar configureFlags --disable-static;
            fi;
        fi;
        if [ -z "${dontPatchShebangsInConfigure:-}" ]; then
            patchShebangs --build "$configureScript";
        fi;
    fi;
    if [ -n "$configureScript" ]; then
        local -a flagsArray;
        concatTo flagsArray configureFlags configureFlagsArray;
        echoCmd 'configure flags' "${flagsArray[@]}";
        $configureScript "${flagsArray[@]}";
        unset flagsArray;
    else
        echo "no configure script, doing nothing";
    fi;
    runHook postConfigure
}
activatePackage ()
{
 
    local pkg="$1";
    local -r hostOffset="$2";
    local -r targetOffset="$3";
    (( hostOffset <= targetOffset )) || exit 1;
    if [ -f "$pkg" ]; then
        nixTalkativeLog "sourcing setup hook '$pkg'";
        source "$pkg";
    fi;
    if [[ -z "${strictDeps-}" || "$hostOffset" -le -1 ]]; then
        addToSearchPath _PATH "$pkg/bin";
    fi;
    if (( hostOffset <= -1 )); then
        addToSearchPath _XDG_DATA_DIRS "$pkg/share";
    fi;
    if [[ "$hostOffset" -eq 0 && -d "$pkg/bin" ]]; then
        addToSearchPath _HOST_PATH "$pkg/bin";
    fi;
    if [[ -f "$pkg/nix-support/setup-hook" ]]; then
        nixTalkativeLog "sourcing setup hook '$pkg/nix-support/setup-hook'";
        source "$pkg/nix-support/setup-hook";
    fi
}
auditTmpdir ()
{
 
    local dir="$1";
    [ -e "$dir" ] || return 0;
    echo "checking for references to $TMPDIR/ in $dir...";
    local tmpdir elf_fifo script_fifo;
    tmpdir="$(mktemp -d)";
    elf_fifo="$tmpdir/elf";
    script_fifo="$tmpdir/script";
    mkfifo "$elf_fifo" "$script_fifo";
    ( find "$dir" -type f -not -path '*/.build-id/*' -print0 | while IFS= read -r -d '' file; do
        if isELF "$file"; then
            printf '%s\0' "$file" 1>&3;
        else
            if isScript "$file"; then
                filename=${file##*/};
                dir=${file%/*};
                if [ -e "$dir/.$filename-wrapped" ]; then
                    printf '%s\0' "$file" 1>&4;
                fi;
            fi;
        fi;
    done;
    exec 3>&- 4>&- ) 3> "$elf_fifo" 4> "$script_fifo" & ( xargs -0 -r -P "$NIX_BUILD_CORES" -n 1 sh -c '
            if { printf :; patchelf --print-rpath "$1"; } | grep -q -F ":$TMPDIR/"; then
                echo "RPATH of binary $1 contains a forbidden reference to $TMPDIR/"
                exit 1
            fi
        ' _ < "$elf_fifo" ) & local pid_elf=$!;
    local pid_script;
    ( xargs -0 -r -P "$NIX_BUILD_CORES" -n 1 sh -c '
            if grep -q -F "$TMPDIR/" "$1"; then
                echo "wrapper script $1 contains a forbidden reference to $TMPDIR/"
                exit 1
            fi
        ' _ < "$script_fifo" ) & local pid_script=$!;
    wait "$pid_elf" || { 
        echo "Some binaries contain forbidden references to $TMPDIR/. Check the error above!";
        exit 1
    };
    wait "$pid_script" || { 
        echo "Some scripts contain forbidden references to $TMPDIR/. Check the error above!";
        exit 1
    };
    rm -r "$tmpdir"
}
isELF ()
{
 
    local fn="$1";
    local fd;
    local magic;
    exec {fd}< "$fn";
    LANG=C read -r -n 4 -u "$fd" magic;
    exec {fd}>&-;
    if [ "$magic" = 'ELF' ]; then
        return 0;
    else
        return 1;
    fi
}
nixChattyLog ()
{
 
    _nixLogWithLevel 5 "$*"
}
noBrokenSymlinksInAllOutputs ()
{
 
    if [[ -z ${dontCheckForBrokenSymlinks-} ]]; then
        for output in $(getAllOutputNames);
        do
            noBrokenSymlinks "${!output}";
        done;
    fi
}
_moveSbin ()
{
 
    if [ "${dontMoveSbin-}" = 1 ]; then
        return;
    fi;
    if [ ! -e "$prefix/sbin" -o -L "$prefix/sbin" ]; then
        return;
    fi;
    echo "moving $prefix/sbin/* to $prefix/bin";
    mkdir -p $prefix/bin;
    shopt -s dotglob;
    for i in $prefix/sbin/*;
    do
        mv "$i" $prefix/bin;
    done;
    shopt -u dotglob;
    rmdir $prefix/sbin;
    ln -s bin $prefix/sbin
}
prependToVar ()
{
 
    local -n nameref="$1";
    local useArray type;
    if [ -n "$__structuredAttrs" ]; then
        useArray=true;
    else
        useArray=false;
    fi;
    if type=$(declare -p "$1" 2> /dev/null); then
        case "${type#* }" in 
            -A*)
                echo "prependToVar(): ERROR: trying to use prependToVar on an associative array." 1>&2;
                return 1
            ;;
            -a*)
                useArray=true
            ;;
            *)
                useArray=false
            ;;
        esac;
    fi;
    shift;
    if $useArray; then
        nameref=("$@" ${nameref+"${nameref[@]}"});
    else
        nameref="$* ${nameref-}";
    fi
}
runPhase ()
{
 
    local curPhase="$*";
    if [[ "$curPhase" = unpackPhase && -n "${dontUnpack:-}" ]]; then
        return;
    fi;
    if [[ "$curPhase" = patchPhase && -n "${dontPatch:-}" ]]; then
        return;
    fi;
    if [[ "$curPhase" = configurePhase && -n "${dontConfigure:-}" ]]; then
        return;
    fi;
    if [[ "$curPhase" = buildPhase && -n "${dontBuild:-}" ]]; then
        return;
    fi;
    if [[ "$curPhase" = checkPhase && -z "${doCheck:-}" ]]; then
        return;
    fi;
    if [[ "$curPhase" = installPhase && -n "${dontInstall:-}" ]]; then
        return;
    fi;
    if [[ "$curPhase" = fixupPhase && -n "${dontFixup:-}" ]]; then
        return;
    fi;
    if [[ "$curPhase" = installCheckPhase && -z "${doInstallCheck:-}" ]]; then
        return;
    fi;
    if [[ "$curPhase" = distPhase && -z "${doDist:-}" ]]; then
        return;
    fi;
    showPhaseHeader "$curPhase";
    dumpVars;
    local startTime endTime;
    startTime=$(date +"%s");
    eval "${!curPhase:-$curPhase}";
    endTime=$(date +"%s");
    showPhaseFooter "$curPhase" "$startTime" "$endTime";
    if [ "$curPhase" = unpackPhase ]; then
        [ -n "${sourceRoot:-}" ] && chmod +x -- "${sourceRoot}";
        cd -- "${sourceRoot:-.}";
    fi
}
substituteInPlace ()
{
 
    local -a fileNames=();
    for arg in "$@";
    do
        if [[ "$arg" = "--"* ]]; then
            break;
        fi;
        fileNames+=("$arg");
        shift;
    done;
    if ! [[ "${#fileNames[@]}" -gt 0 ]]; then
        echo "substituteInPlace called without any files to operate on (files must come before options!)" 1>&2;
        return 1;
    fi;
    for file in "${fileNames[@]}";
    do
        substitute "$file" "$file" "$@";
    done
}
definePhases ()
{
 
    if [ -z "${phases[*]:-}" ]; then
        phases="${prePhases[*]:-} unpackPhase patchPhase ${preConfigurePhases[*]:-}             configurePhase ${preBuildPhases[*]:-} buildPhase checkPhase             ${preInstallPhases[*]:-} installPhase ${preFixupPhases[*]:-} fixupPhase installCheckPhase             ${preDistPhases[*]:-} distPhase ${postPhases[*]:-}";
    fi
}
runOneHook ()
{
 
    local hookName="$1";
    shift;
    local hooksSlice="${hookName%Hook}Hooks[@]";
    local hook ret=1;
    for hook in "_callImplicitHook 1 $hookName" ${!hooksSlice+"${!hooksSlice}"};
    do
        _logHook "$hookName" "$hook" "$@";
        if _eval "$hook" "$@"; then
            ret=0;
            break;
        fi;
    done;
    return "$ret"
}
addToSearchPath ()
{
 
    addToSearchPathWithCustomDelimiter ":" "$@"
}
isScript ()
{
 
    local fn="$1";
    local fd;
    local magic;
    exec {fd}< "$fn";
    LANG=C read -r -n 2 -u "$fd" magic;
    exec {fd}>&-;
    if [[ "$magic" =~ \#! ]]; then
        return 0;
    else
        return 1;
    fi
}
substituteStream ()
{
 
    local var=$1;
    local description=$2;
    shift 2;
    while (( "$#" )); do
        local replace_mode="$1";
        case "$1" in 
            --replace)
                if ! "$_substituteStream_has_warned_replace_deprecation"; then
                    echo "substituteStream() in derivation $name: WARNING: '--replace' is deprecated, use --replace-{fail,warn,quiet}. ($description)" 1>&2;
                    _substituteStream_has_warned_replace_deprecation=true;
                fi;
                replace_mode='--replace-warn'
            ;&
            --replace-quiet | --replace-warn | --replace-fail)
                pattern="$2";
                replacement="$3";
                shift 3;
                if ! [[ "${!var}" == *"$pattern"* ]]; then
                    if [ "$replace_mode" == --replace-warn ]; then
                        printf "substituteStream() in derivation $name: WARNING: pattern %q doesn't match anything in %s\n" "$pattern" "$description" 1>&2;
                    else
                        if [ "$replace_mode" == --replace-fail ]; then
                            printf "substituteStream() in derivation $name: ERROR: pattern %q doesn't match anything in %s\n" "$pattern" "$description" 1>&2;
                            return 1;
                        fi;
                    fi;
                fi;
                eval "$var"'=${'"$var"'//"$pattern"/"$replacement"}'
            ;;
            --subst-var)
                local varName="$2";
                shift 2;
                if ! [[ "$varName" =~ ^[a-zA-Z_][a-zA-Z0-9_]*$ ]]; then
                    echo "substituteStream() in derivation $name: ERROR: substitution variables must be valid Bash names, \"$varName\" isn't." 1>&2;
                    return 1;
                fi;
                if [ -z ${!varName+x} ]; then
                    echo "substituteStream() in derivation $name: ERROR: variable \$$varName is unset" 1>&2;
                    return 1;
                fi;
                pattern="@$varName@";
                replacement="${!varName}";
                eval "$var"'=${'"$var"'//"$pattern"/"$replacement"}'
            ;;
            --subst-var-by)
                pattern="@$2@";
                replacement="$3";
                eval "$var"'=${'"$var"'//"$pattern"/"$replacement"}';
                shift 3
            ;;
            *)
                echo "substituteStream() in derivation $name: ERROR: Invalid command line argument: $1" 1>&2;
                return 1
            ;;
        esac;
    done;
    printf "%s" "${!var}"
}
echoCmd ()
{
 
    printf "%s:" "$1";
    shift;
    printf ' %q' "$@";
    echo
}
updateSourceDateEpoch ()
{
 
    local path="$1";
    [[ $path == -* ]] && path="./$path";
    local -a res=($(find "$path" -type f -not -newer "$NIX_BUILD_TOP/.." -printf '%T@ "%p"\0' | sort -n --zero-terminated | tail -n1 --zero-terminated | head -c -1));
    local time="${res[0]//\.[0-9]*/}";
    local newestFile="${res[1]}";
    if [ "${time:-0}" -gt "$SOURCE_DATE_EPOCH" ]; then
        echo "setting SOURCE_DATE_EPOCH to timestamp $time of file $newestFile";
        export SOURCE_DATE_EPOCH="$time";
        local now="$(date +%s)";
        if [ "$time" -gt $((now - 60)) ]; then
            echo "warning: file $newestFile may be generated; SOURCE_DATE_EPOCH may be non-deterministic";
        fi;
    fi
}
PATH="$PATH${nix_saved_PATH:+:$nix_saved_PATH}"
XDG_DATA_DIRS="$XDG_DATA_DIRS${nix_saved_XDG_DATA_DIRS:+:$nix_saved_XDG_DATA_DIRS}"

eval "${shellHook:-}"
shopt -s expand_aliases

exec cargo --version