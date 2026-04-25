package sh.droidscope.agent

import org.gradle.tooling.GradleConnector
import org.gradle.tooling.events.OperationType
import org.gradle.tooling.events.ProgressListener
import org.gradle.tooling.events.task.TaskFinishEvent
import org.gradle.tooling.events.task.TaskStartEvent
import org.gradle.tooling.events.task.TaskSuccessResult
import org.gradle.tooling.events.task.TaskFailureResult
import org.gradle.tooling.events.task.TaskSkippedResult
import org.gradle.tooling.model.GradleProject
import java.io.File
import java.io.PrintStream
import java.time.Instant
import kotlin.system.exitProcess

/**
 * Streams Gradle task progress events as newline-delimited JSON on stdout.
 * Consumed by the Rust TUI via `std::process::Command` piped stdout.
 */
fun main(args: Array<String>) {
    val opts = parseArgs(args)
    val projectDir = File(opts.project)
    if (!projectDir.isDirectory) {
        emitError("project directory not found: ${opts.project}")
        exitProcess(2)
    }

    // Redirect stderr to avoid Tooling API writing on our event stream.
    val out: PrintStream = System.out
    val originalErr = System.err
    System.setErr(PrintStream(File.createTempFile("gradle-agent", ".log").also { it.deleteOnExit() }))

    val connector = GradleConnector.newConnector().forProjectDirectory(projectDir)
    val connection = connector.connect()

    try {
        if (opts.listVariants) {
            try {
                val gp = connection.getModel(GradleProject::class.java)
                val variants = collectVariants(gp)
                emit(out, variantsJson(variants))
            } catch (e: Exception) {
                emitError(e.message ?: e.javaClass.simpleName)
                exitProcess(1)
            }
            return
        }
        val launcher = connection.newBuild()
            .forTasks(*opts.tasks.toTypedArray())
            .addProgressListener(ProgressListener { event ->
                when (event) {
                    is TaskStartEvent -> emit(out, taskStartJson(event))
                    is TaskFinishEvent -> emit(out, taskFinishJson(event))
                    else -> {}
                }
            }, setOf(OperationType.TASK))

        try {
            launcher.run()
            emit(out, buildFinishJson("SUCCESS"))
        } catch (e: Exception) {
            emit(out, buildFinishJson("FAILED"))
            emitError(e.message ?: e.javaClass.simpleName)
            exitProcess(1)
        }
    } finally {
        System.setErr(originalErr)
        connection.close()
    }
}

private data class Options(val project: String, val tasks: List<String>, val listVariants: Boolean)

private fun parseArgs(args: Array<String>): Options {
    var project: String? = null
    var listVariants = false
    val tasks = mutableListOf<String>()
    var i = 0
    while (i < args.size) {
        when (args[i]) {
            "--project" -> { project = args[++i] }
            "--task" -> { tasks.add(args[++i]) }
            "--list-variants" -> { listVariants = true }
            else -> { tasks.add(args[i]) }
        }
        i++
    }
    if (project == null) {
        emitError("missing --project <dir>")
        exitProcess(2)
    }
    if (!listVariants && tasks.isEmpty()) tasks.add("help")
    return Options(project, tasks, listVariants)
}

private val VARIANT_REGEX = Regex("^assemble([A-Z][A-Za-z0-9]*)$")
private val VARIANT_EXCLUDE_SUFFIXES = listOf("AndroidTest", "UnitTest", "TestFixtures")

private fun collectVariants(project: GradleProject): List<String> {
    val out = sortedSetOf<String>()
    walkVariants(project, out)
    return out.toList()
}

private fun walkVariants(project: GradleProject, out: MutableSet<String>) {
    for (task in project.tasks) {
        val match = VARIANT_REGEX.matchEntire(task.name) ?: continue
        val name = match.groupValues[1]
        if (VARIANT_EXCLUDE_SUFFIXES.any { name.endsWith(it) }) continue
        out.add(name.replaceFirstChar { it.lowercaseChar() })
    }
    for (child in project.children) walkVariants(child, out)
}

private fun variantsJson(items: List<String>): String {
    val sb = StringBuilder("""{"kind":"variants","ts":"${Instant.now()}","items":[""")
    items.forEachIndexed { i, v ->
        if (i > 0) sb.append(',')
        sb.append(quote(v))
    }
    sb.append("]}")
    return sb.toString()
}

private fun emit(out: PrintStream, json: String) {
    synchronized(out) {
        out.println(json)
        out.flush()
    }
}

private fun emitError(msg: String) {
    val json = """{"kind":"error","ts":"${Instant.now()}","message":${quote(msg)}}"""
    emit(System.out, json)
}

private fun taskStartJson(event: TaskStartEvent): String =
    """{"kind":"task_start","ts":"${Instant.ofEpochMilli(event.eventTime)}","path":${quote(event.descriptor.taskPath)}}"""

private fun taskFinishJson(event: TaskFinishEvent): String {
    val outcome = when (val r = event.result) {
        is TaskSkippedResult -> "SKIPPED"
        is TaskSuccessResult -> when {
            r.isFromCache -> "FROM_CACHE"
            r.isUpToDate -> "UP_TO_DATE"
            else -> "SUCCESS"
        }
        is TaskFailureResult -> "FAILED"
        else -> "UNKNOWN"
    }
    val duration = event.result.endTime - event.result.startTime
    return """{"kind":"task_finish","ts":"${Instant.ofEpochMilli(event.eventTime)}","path":${quote(event.descriptor.taskPath)},"outcome":"$outcome","duration_ms":$duration}"""
}

private fun buildFinishJson(outcome: String): String =
    """{"kind":"build_finish","ts":"${Instant.now()}","outcome":"$outcome"}"""

private fun quote(s: String): String {
    val sb = StringBuilder("\"")
    for (c in s) {
        when (c) {
            '\\' -> sb.append("\\\\")
            '"' -> sb.append("\\\"")
            '\n' -> sb.append("\\n")
            '\r' -> sb.append("\\r")
            '\t' -> sb.append("\\t")
            else -> if (c.code < 0x20) sb.append(String.format("\\u%04x", c.code)) else sb.append(c)
        }
    }
    sb.append('"')
    return sb.toString()
}
