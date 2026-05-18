// Ghidra headless Java GhidraScript for PanelDriver.sys analysis.
// Output goes to stdout. Run via:
//   analyzeHeadless <project> <name> -process PanelDriver.sys -noanalysis \
//       -scriptPath <dir> -postScript PanelDriverDump.java
//
// @category W767
//@runtime Java

import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileOptions;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.program.model.symbol.ExternalLocation;
import ghidra.program.model.symbol.ExternalLocationIterator;
import ghidra.program.model.symbol.ExternalManager;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceManager;
import ghidra.program.model.symbol.Symbol;
import ghidra.util.task.ConsoleTaskMonitor;

import java.util.ArrayList;
import java.util.List;

public class PanelDriverDump extends GhidraScript {

    @Override
    public void run() throws Exception {
        println("=".repeat(80));
        println("PanelDriver.sys analysis");
        println("=".repeat(80));
        println("Program:    " + currentProgram.getName());
        println("Compiler:   " + currentProgram.getCompiler());
        println("Image base: " + currentProgram.getImageBase());
        println("Memory blocks:");
        for (MemoryBlock blk : currentProgram.getMemory().getBlocks()) {
            String perms = (blk.isExecute() ? "x" : "") +
                           (blk.isWrite()   ? "w" : "") +
                           (blk.isRead()    ? "r" : "");
            println(String.format("  %-22s %s..%s   %s",
                blk.getName(), blk.getStart(), blk.getEnd(), perms));
        }
        println("");

        // --- 1. Functions ---
        println("=".repeat(80));
        println("FUNCTIONS");
        println("=".repeat(80));
        FunctionManager fm = currentProgram.getFunctionManager();
        List<Function> functions = new ArrayList<>();
        for (Function f : fm.getFunctions(true)) functions.add(f);
        println("Total: " + functions.size());
        for (Function f : functions) {
            println(String.format("  %s  %s  (params: %d, body: %d bytes)",
                f.getEntryPoint(), f.getName(), f.getParameterCount(),
                f.getBody().getNumAddresses()));
        }
        println("");

        // --- 2. External imports + ref counts ---
        println("=".repeat(80));
        println("EXTERNAL IMPORTS");
        println("=".repeat(80));
        ExternalManager extm = currentProgram.getExternalManager();
        for (String lib : extm.getExternalLibraryNames()) {
            println("");
            println("-- from " + lib + " --");
            ExternalLocationIterator it = extm.getExternalLocations(lib);
            while (it.hasNext()) {
                ExternalLocation loc = it.next();
                Symbol sym = loc.getSymbol();
                int rc = 0;
                if (sym != null) {
                    for (Reference r : sym.getReferences(monitor)) rc++;
                }
                println(String.format("  %-40s   (refs: %d)", loc.getLabel(), rc));
            }
        }
        println("");

        // --- 3. Find string references ---
        println("=".repeat(80));
        println("STRING REFERENCES");
        println("=".repeat(80));
        String[] needles = { "GFTV", "AeiB", "AeoB", "AuxRead", "AuxWrite",
                             "displayId", "numBytes", "bufferData", "bufferSize" };
        Memory mem = currentProgram.getMemory();
        ReferenceManager rm = currentProgram.getReferenceManager();
        ConsoleTaskMonitor mon = new ConsoleTaskMonitor();
        for (String needle : needles) {
            println("");
            println("-- searching for '" + needle + "' --");
            byte[] bytes = needle.getBytes("ASCII");
            Address found = mem.findBytes(currentProgram.getMinAddress(), bytes,
                null, true, mon);
            if (found == null) {
                println("  (not found)");
                continue;
            }
            println("  Found at: " + found);
            for (Reference r : rm.getReferencesTo(found)) {
                println("  <- ref from " + r.getFromAddress() +
                        " (" + r.getReferenceType() + ")");
            }
        }
        println("");

        // --- 4. Decompile every function ---
        println("=".repeat(80));
        println("DECOMPILATION");
        println("=".repeat(80));
        DecompInterface ifc = new DecompInterface();
        DecompileOptions opt = new DecompileOptions();
        ifc.setOptions(opt);
        ifc.openProgram(currentProgram);
        for (Function f : functions) {
            if (f.isThunk() || f.isExternal()) continue;
            println("");
            println("#".repeat(60));
            println("# function: " + f.getName() + " @ " + f.getEntryPoint());
            println("#".repeat(60));
            try {
                DecompileResults res = ifc.decompileFunction(f, 60, mon);
                if (res == null || !res.decompileCompleted()) {
                    println("  (decompile failed)");
                    continue;
                }
                String c = res.getDecompiledFunction() != null
                    ? res.getDecompiledFunction().getC() : null;
                if (c == null) { println("  (no decomp)"); continue; }
                println(c);
            } catch (Exception e) {
                println("  (exception: " + e + ")");
            }
        }

        println("");
        println("=".repeat(80));
        println("END");
        println("=".repeat(80));
    }
}
