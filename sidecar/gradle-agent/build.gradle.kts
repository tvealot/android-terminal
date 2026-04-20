plugins {
    kotlin("jvm") version "1.9.25"
    application
}

group = "sh.droidscope"
version = "0.1.0"

repositories {
    mavenCentral()
    maven("https://repo.gradle.org/gradle/libs-releases")
}

dependencies {
    implementation("org.gradle:gradle-tooling-api:8.10")
    runtimeOnly("org.slf4j:slf4j-simple:2.0.13")
}

application {
    mainClass.set("sh.droidscope.agent.AgentKt")
}

kotlin {
    jvmToolchain(17)
}

tasks.jar {
    archiveBaseName.set("gradle-agent")
    manifest {
        attributes["Main-Class"] = "sh.droidscope.agent.AgentKt"
    }
    // Build a fat jar so the Rust side can launch with `java -jar gradle-agent.jar`.
    from({
        configurations.runtimeClasspath.get().map { if (it.isDirectory) it else zipTree(it) }
    }) {
        exclude("META-INF/*.SF", "META-INF/*.DSA", "META-INF/*.RSA")
    }
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE
}
