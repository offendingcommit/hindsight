package io.vectorize.hindsight.android

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.io.FileOutputStream

/**
 * Instrumented test: proves PostgreSQL + pgvector runs on Android ARM64.
 *
 * PG binaries are bundled as .so files in jniLibs (for SELinux exec permission)
 * and shared libraries/data files are in assets.
 */
@RunWith(AndroidJUnit4::class)
class PostgresTest {

    companion object {
        private const val TAG = "PostgresTest"
    }

    @Test
    fun testPostgresWithPgvector() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val baseDir = context.filesDir.absolutePath
        val nativeDir = context.applicationInfo.nativeLibraryDir
        val dataDir = "$baseDir/pg-data"
        val logFile = "$baseDir/pg.log"
        val tmpDir = "$baseDir/tmp"

        Log.i(TAG, "Native lib dir: $nativeDir")
        Log.i(TAG, "Files dir: $baseDir")

        // Step 1: Find PG binaries (installed as .so in native lib dir)
        val postgresBin = "$nativeDir/libpostgres.so"
        val initdbBin = "$nativeDir/libinitdb.so"
        val createdbBin = "$nativeDir/libcreatedb.so"
        val psqlBin = "$nativeDir/libpsql.so"
        val pgIsReadyBin = "$nativeDir/libpg_isready.so"
        val pgCtlBin = "$nativeDir/libpg_ctl.so"

        assertTrue("postgres binary not found at $postgresBin", File(postgresBin).exists())
        Log.i(TAG, "Step 1: PG binaries found in native lib dir")

        // Copy binaries with correct names so initdb can find postgres in same dir
        val pgBinDir = "$baseDir/pg-bin"
        File(pgBinDir).deleteRecursively()
        File(pgBinDir).mkdirs()
        mapOf(
            "postgres" to "libpostgres.so",
            "initdb" to "libinitdb.so",
            "createdb" to "libcreatedb.so",
            "psql" to "libpsql.so",
            "pg_isready" to "libpg_isready.so",
            "pg_ctl" to "libpg_ctl.so"
        ).forEach { (name, soName) ->
            val source = File("$nativeDir/$soName")
            val dest = File("$pgBinDir/$name")
            if (source.exists()) {
                source.inputStream().use { inp ->
                    FileOutputStream(dest).use { out -> inp.copyTo(out) }
                }
                dest.setExecutable(true, false)
                Log.i(TAG, "  Copied $soName -> $name (${dest.length()} bytes)")
            } else {
                Log.w(TAG, "  Missing: $soName in $nativeDir")
            }
        }

        // Step 2: Extract shared libs from assets to a writable location
        Log.i(TAG, "Step 2: Extracting shared libraries from assets...")
        val pgLibDir = "$baseDir/pg-lib"
        val pgShareDir = "$baseDir/pg-share"
        File(pgLibDir).mkdirs()
        File(pgShareDir).mkdirs()
        File(tmpDir).mkdirs()

        // Extract the tar from assets (contains lib/ and share/)
        val archivePath = "$baseDir/postgres-arm64.tar"
        context.assets.open("postgres-arm64.tar").use { input ->
            FileOutputStream(archivePath).use { output ->
                input.copyTo(output)
            }
        }

        val extractResult = exec(
            "tar", "xf", archivePath, "-C", baseDir,
            env = emptyMap()
        )
        Log.i(TAG, "Extract: ${extractResult.take(200)}")

        // Set up LD_LIBRARY_PATH to include both native dir and extracted libs
        val ldPath = "$nativeDir:$baseDir/lib"

        // Step 3: Check postgres version (using correctly-named copy)
        val versionResult = exec("$pgBinDir/postgres", "--version", env = mapOf("LD_LIBRARY_PATH" to ldPath))
        Log.i(TAG, "Step 3: $versionResult")
        assertTrue("postgres --version failed: $versionResult", versionResult.contains("PostgreSQL"))

        // Step 4: Run initdb
        Log.i(TAG, "Step 4: Running initdb...")
        File(dataDir).deleteRecursively()
        File(dataDir).mkdirs()

        // Tell initdb where to find postgres binary and share data
        val initdbEnv = mapOf(
            "LD_LIBRARY_PATH" to ldPath,
            "TMPDIR" to tmpDir,
            "PGDATA" to dataDir
        )

        // Test: can postgres run with shared memory?
        val checkScript = File("$pgBinDir/run_check.sh")
        checkScript.writeText("""#!/system/bin/sh
export LD_LIBRARY_PATH=$ldPath
export TMPDIR=$tmpDir
mkdir -p $dataDir
# Test if postgres can allocate shared memory (the initdb probe does this)
$pgBinDir/postgres -C max_connections 2>&1 || true
# Try starting postgres briefly to test shm
$pgBinDir/postgres --single -D $dataDir -c shared_buffers=8MB -c dynamic_shared_memory_type=mmap postgres < /dev/null 2>&1 || true
echo "POSTGRES_CHECK_EXIT=$$?"
""")
        checkScript.setExecutable(true, false)
        val checkResult = exec("/system/bin/sh", checkScript.absolutePath, env = initdbEnv, timeoutMs = 30_000)
        Log.i(TAG, "postgres --check result: ${checkResult.take(500)}")

        // Create a wrapper script so LD_LIBRARY_PATH persists to child processes
        val wrapperScript = File("$pgBinDir/run_initdb.sh")
        wrapperScript.writeText("""#!/system/bin/sh
export LD_LIBRARY_PATH=$ldPath
export TMPDIR=$tmpDir
$pgBinDir/initdb -D $dataDir -L $baseDir/share/postgresql --auth=trust --username=hindsight --no-locale -c max_connections=10 -c shared_buffers=16MB -c dynamic_shared_memory_type=mmap --no-clean 2>&1
echo "EXIT_CODE=$$?"
""")
        wrapperScript.setExecutable(true, false)

        val initdbResult = exec(
            "/system/bin/sh", wrapperScript.absolutePath,
            env = initdbEnv,
            timeoutMs = 480_000  // initdb probe can take several minutes on virtual devices
        )
        Log.i(TAG, "initdb result (${initdbResult.length} chars): ${initdbResult.take(500)}")

        // Check if pg_control was created
        val pgControlExists = File("$dataDir/global/pg_control").exists()
        Log.i(TAG, "pg_control exists: $pgControlExists")

        // If initdb failed, check what files exist
        if (!pgControlExists) {
            val dataFiles = File(dataDir).listFiles()?.map { it.name } ?: emptyList()
            Log.i(TAG, "pg-data contents: $dataFiles")
            val globalFiles = File("$dataDir/global").listFiles()?.map { it.name } ?: emptyList()
            Log.i(TAG, "pg-data/global contents: $globalFiles")
            // Check if share dir exists and has bki
            val shareExists = File("$baseDir/share/postgresql/postgres.bki").exists()
            Log.i(TAG, "postgres.bki exists: $shareExists")
        }
        assertTrue("initdb failed - pg_control missing. Output: ${initdbResult.take(200)}", pgControlExists)

        // Step 5: Start PostgreSQL
        Log.i(TAG, "Step 5: Starting PostgreSQL...")
        val pgEnv = mapOf(
            "LD_LIBRARY_PATH" to ldPath,
            "TMPDIR" to tmpDir
        )

        // Start postgres directly (pg_ctl may not find the binary)
        val pgProcess = ProcessBuilder(
            "$pgBinDir/postgres",
            "-D", dataDir,
            "-k", tmpDir,
            "-p", "15432",
            "-c", "listen_addresses="
        ).apply {
            environment().putAll(pgEnv)
            redirectErrorStream(true)
            redirectOutput(File(logFile))
        }.start()

        // Wait for PG to be ready
        var pgReady = false
        for (i in 1..30) {
            Thread.sleep(1000)
            val isReady = exec(
                "$pgBinDir/pg_isready", "-h", tmpDir, "-p", "15432",
                env = pgEnv
            )
            if (isReady.contains("accepting connections")) {
                pgReady = true
                Log.i(TAG, "PostgreSQL ready after ${i}s")
                break
            }
        }

        if (!pgReady) {
            val pgLog = try { File(logFile).readText() } catch (_: Exception) { "no log" }
            Log.e(TAG, "PG log: $pgLog")
        }
        assertTrue("PostgreSQL failed to start", pgReady)

        try {
            // Step 6: Create database
            Log.i(TAG, "Step 6: Creating database...")
            exec("$pgBinDir/createdb", "-h", tmpDir, "-p", "15432", "-U", "hindsight", "hindsight", env = pgEnv)

            // Step 7: Test pgvector
            Log.i(TAG, "Step 7: Testing pgvector...")
            val vectorResult = exec(
                "$pgBinDir/psql", "-h", tmpDir, "-p", "15432", "-U", "hindsight", "-d", "hindsight",
                "-c", "CREATE EXTENSION IF NOT EXISTS vector; SELECT '[1,2,3]'::vector <-> '[4,5,6]'::vector AS distance;",
                env = pgEnv
            )
            Log.i(TAG, "pgvector result: $vectorResult")
            assertTrue("pgvector failed: $vectorResult", vectorResult.contains("5.196"))

            Log.i(TAG, "=== SUCCESS: PostgreSQL 18 + pgvector works on Android ARM64! ===")
        } finally {
            pgProcess.destroy()
            pgProcess.waitFor()
            Log.i(TAG, "PostgreSQL stopped")
        }
    }

    private fun exec(
        vararg command: String,
        env: Map<String, String>,
        timeoutMs: Long = 60_000
    ): String {
        val pb = ProcessBuilder(*command)
        pb.redirectErrorStream(true)
        pb.environment().putAll(env)

        env["TMPDIR"]?.let { File(it).mkdirs() }

        val process = pb.start()

        // Read output in a separate thread to avoid blocking
        val output = StringBuilder()
        val readerThread = Thread {
            process.inputStream.bufferedReader().forEachLine { line ->
                output.appendLine(line)
            }
        }
        readerThread.isDaemon = true
        readerThread.start()

        val completed = process.waitFor(timeoutMs, java.util.concurrent.TimeUnit.MILLISECONDS)
        if (!completed) {
            Log.w(TAG, "Command timed out after ${timeoutMs}ms: ${command.joinToString(" ")}")
            process.destroyForcibly()
        } else {
            Log.i(TAG, "Command exited with code ${process.exitValue()}: ${command.first().substringAfterLast('/')}")
        }
        readerThread.join(2000)

        return output.toString()
    }
}
