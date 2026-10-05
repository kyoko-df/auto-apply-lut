// Parse executable metadata without depending on otool, dumpbin, Python or Homebrew.
// Standalone releases deliberately reject non-system dylibs/DLLs: copying a
// package-manager executable into resources is not a self-contained FFmpeg.
const CPU = new Map([[0x01000007, 'x86_64'], [0x0100000c, 'aarch64']]);
const SYSTEM_DLLS = new Set([
  'advapi32.dll', 'avrt.dll', 'bcrypt.dll', 'bcryptprimitives.dll', 'cabinet.dll', 'cfgmgr32.dll',
  'comctl32.dll', 'comdlg32.dll', 'crypt32.dll', 'd3d11.dll', 'd3d12.dll', 'd3d9.dll',
  'dbghelp.dll', 'dnsapi.dll', 'dsound.dll', 'dwmapi.dll', 'dxgi.dll', 'dxva2.dll',
  'gdi32.dll', 'imm32.dll', 'iphlpapi.dll', 'kernel32.dll', 'kernelbase.dll', 'mf.dll',
  'mfplat.dll', 'mfreadwrite.dll', 'mfuuid.dll', 'msvcrt.dll', 'ncrypt.dll', 'netapi32.dll',
  'normaliz.dll', 'ntdll.dll', 'ole32.dll', 'oleaut32.dll', 'powrprof.dll', 'propsys.dll',
  'psapi.dll', 'rpcrt4.dll', 'secur32.dll', 'setupapi.dll', 'shcore.dll', 'shell32.dll',
  'shlwapi.dll', 'strmiids.dll', 'user32.dll', 'userenv.dll', 'usp10.dll', 'version.dll',
  'vfw32.dll', 'winhttp.dll', 'wininet.dll', 'winmm.dll', 'winspool.drv', 'wintrust.dll',
  'ws2_32.dll', 'wtsapi32.dll', 'ucrtbase.dll',
]);

function checkRange(buffer, offset, length) {
  if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(length) || offset < 0 || length < 0 || offset + length > buffer.length) {
    throw new Error('Malformed executable: metadata is outside the file.');
  }
}

function cstring(buffer, offset, limit = buffer.length) {
  checkRange(buffer, offset, 1);
  const end = buffer.indexOf(0, offset);
  if (end < 0 || end >= limit) throw new Error('Malformed executable: unterminated dependency name.');
  return buffer.toString('utf8', offset, end);
}

function machoSlice(buffer) {
  checkRange(buffer, 0, 32);
  if (buffer.readUInt32LE(0) !== 0xfeedfacf) throw new Error('Only 64-bit little-endian Mach-O binaries are supported.');
  const arch = CPU.get(buffer.readUInt32LE(4));
  if (!arch) throw new Error('Unsupported Mach-O architecture.');
  if (buffer.readUInt32LE(12) !== 2) throw new Error('Expected a Mach-O executable.');
  const count = buffer.readUInt32LE(16);
  const commandsEnd = 32 + buffer.readUInt32LE(20);
  checkRange(buffer, 32, commandsEnd - 32);
  const dependencies = [];
  let offset = 32;
  for (let i = 0; i < count; i += 1) {
    if (offset + 8 > commandsEnd) throw new Error('Malformed Mach-O load commands.');
    const command = buffer.readUInt32LE(offset) & 0x7fffffff;
    const size = buffer.readUInt32LE(offset + 4);
    if (size < 8 || offset + size > commandsEnd) throw new Error('Malformed Mach-O command size.');
    // LC_LOAD_DYLIB, LC_LOAD_WEAK_DYLIB, LC_REEXPORT_DYLIB,
    // LC_LAZY_LOAD_DYLIB and LC_LOAD_UPWARD_DYLIB all load external code.
    if ([0xc, 0x18, 0x1f, 0x20, 0x23].includes(command)) {
      if (size < 24) throw new Error('Malformed Mach-O dylib command.');
      const nameOffset = buffer.readUInt32LE(offset + 8);
      if (nameOffset < 24 || nameOffset >= size) throw new Error('Malformed Mach-O dependency name.');
      dependencies.push(cstring(buffer, offset + nameOffset, offset + size));
    }
    offset += size;
  }
  if (offset !== commandsEnd) throw new Error('Malformed Mach-O command table length.');
  return { arch, dependencies };
}

function macho(buffer) {
  const magic = buffer.readUInt32BE(0);
  if (magic !== 0xcafebabe && magic !== 0xcafebabf) return { platform: 'darwin', slices: [machoSlice(buffer)] };
  checkRange(buffer, 0, 8);
  const count = buffer.readUInt32BE(4);
  const stride = magic === 0xcafebabf ? 32 : 20;
  if (count < 1 || count > 16) throw new Error('Malformed universal Mach-O slice count.');
  checkRange(buffer, 8, count * stride);
  const slices = [];
  for (let i = 0; i < count; i += 1) {
    const start = 8 + i * stride;
    const offset = stride === 32 ? Number(buffer.readBigUInt64BE(start + 8)) : buffer.readUInt32BE(start + 8);
    const size = stride === 32 ? Number(buffer.readBigUInt64BE(start + 16)) : buffer.readUInt32BE(start + 12);
    checkRange(buffer, offset, size);
    const slice = machoSlice(buffer.subarray(offset, offset + size));
    if (CPU.get(buffer.readUInt32BE(start)) !== slice.arch) throw new Error('Mach-O fat header architecture does not match its slice.');
    slices.push(slice);
  }
  return { platform: 'darwin', slices };
}

function pe(buffer) {
  checkRange(buffer, 0, 64);
  const header = buffer.readUInt32LE(0x3c);
  checkRange(buffer, header, 24);
  if (buffer.toString('ascii', header, header + 4) !== 'PE\0\0') throw new Error('Malformed PE signature.');
  if (buffer.readUInt16LE(header + 4) !== 0x8664) throw new Error('Only Windows x86_64 executables are supported.');
  const characteristics = buffer.readUInt16LE(header + 22);
  if (!(characteristics & 2) || characteristics & 0x2000) throw new Error('Expected a Windows executable, not a DLL.');
  const sectionCount = buffer.readUInt16LE(header + 6);
  const optionalSize = buffer.readUInt16LE(header + 20);
  const optional = header + 24;
  checkRange(buffer, optional, optionalSize);
  if (optionalSize < 112 || buffer.readUInt16LE(optional) !== 0x20b) throw new Error('Expected a PE32+ optional header.');
  const directoryCount = buffer.readUInt32LE(optional + 108);
  if (directoryCount > 16 || 112 + directoryCount * 8 > optionalSize) throw new Error('Malformed PE data directories.');
  const sections = [];
  checkRange(buffer, optional + optionalSize, sectionCount * 40);
  for (let i = 0; i < sectionCount; i += 1) {
    const offset = optional + optionalSize + i * 40;
    sections.push({ rva: buffer.readUInt32LE(offset + 12), size: buffer.readUInt32LE(offset + 16), raw: buffer.readUInt32LE(offset + 20) });
  }
  const resolveRva = (rva, length = 1) => {
    const section = sections.find((item) => rva >= item.rva && rva + length <= item.rva + item.size);
    if (!section) throw new Error('Malformed PE import RVA.');
    const offset = section.raw + rva - section.rva;
    checkRange(buffer, offset, length);
    return offset;
  };
  const dependencies = [];
  // Import table and delay import table: both can introduce dependencies.
  for (const [index, stride, nameField] of [[1, 20, 12], [13, 32, 4]]) {
    if (directoryCount <= index) continue;
    const rva = buffer.readUInt32LE(optional + 112 + index * 8);
    const size = buffer.readUInt32LE(optional + 116 + index * 8);
    if (!rva && !size) continue;
    if (!rva || size < stride) throw new Error('Malformed PE import directory.');
    const start = resolveRva(rva, size);
    let terminated = false;
    for (let offset = start; offset + stride <= start + size; offset += stride) {
      if (buffer.subarray(offset, offset + stride).every((byte) => byte === 0)) { terminated = true; break; }
      if (index === 13 && buffer.readUInt32LE(offset) !== 1) throw new Error('Unsupported PE delay import addressing.');
      dependencies.push(cstring(buffer, resolveRva(buffer.readUInt32LE(offset + nameField))));
    }
    if (!terminated) throw new Error('Malformed PE import table termination.');
  }
  return { platform: 'win32', slices: [{ arch: 'x86_64', dependencies }] };
}

export function inspectBinary(buffer) {
  checkRange(buffer, 0, 4);
  if (buffer.toString('ascii', 0, 2) === 'MZ') return pe(buffer);
  if ([0xcffaedfe, 0xcafebabe, 0xcafebabf].includes(buffer.readUInt32BE(0))) return macho(buffer);
  throw new Error('Expected a native Mach-O or PE executable; scripts and package-manager wrappers cannot be bundled.');
}

export function validateDependencies(info) {
  for (const slice of info.slices) {
    for (const dependency of slice.dependencies) {
      const valid = info.platform === 'darwin'
        ? dependency.startsWith('/usr/lib/') || dependency.startsWith('/System/Library/Frameworks/')
        : SYSTEM_DLLS.has(dependency.toLowerCase()) || /^(?:api-ms-win-|ext-ms-win-)[a-z0-9-]+\.dll$/i.test(dependency);
      // Normalize path components before accepting an apparent system dylib.
      if (!valid || dependency.split(/[\\/]/).includes('..')) {
        throw new Error(`Non-system dependency (${slice.arch}): ${dependency}. Use a standalone/static FFmpeg build, not a Homebrew/shared build.`);
      }
    }
  }
}
