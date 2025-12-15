

## Abordar implementacion epica
ok continua con la implementacion de las historias de usuario con enfoque: 
- TDD y tests en verde de toda la aplicacion, commits por historias de usuario implementadas con su testing y siempre deben pasar en verde al finalizar la historia de usuario. 

- Checks visibles de validacion en los documentos para validar que se ha procesado y actualizado la historia de usuario. 
- No romper con el enfoque arquitectura DDD y el uso de buenas practicas de desarrollo.
- No procrastines y siempre haz la implementacion completa de cada historia de usuario con idea de hacerla siempre production ready.
- Intenta no dejar warnings en la compilacion de rust en el codigo.
- Utiliza siempre las ultimas versiones de las dependencias y librerias crates estables.
- Si te encuentras con problemas, busca soluciones en la comunidad de desarrollo o en la documentacion oficial, revisando el codigo directamente del crate a usar, con perplexity o context7.
- Muy importante: Implementa todo el codigo con foco a despliegue en produccion, codigo valido y seguro, no quiero imlementaciones inMemory en codigo productivo, no simulaciones ni funciones con respuesta inmediata y falseadas. El codigo debe ser production ready.
- No se toca las interfaces y ports a la ligera porque eso implica que se rompa el codigo de las implementaciones de todos los adaptadores. Cualquier cambio en las interfaces y ports debe ser bien pensado y valorado antes de realizarlo.
- Cuando crees funcionalidad nueva o actualizada aplica estas buenas practicas:
 1. The Builder Pattern (El Constructor) Para crear objetos complejos con muchas configuraciones opcionales, se usa el Builder.
 2. The Newtype Pattern (Tipado Fuerte) Para crear tipos personalizados que encapsulan un tipo existente y proporcionan una interfaz más específica y segura Value Object.
 3. Type State Pattern (Estado en el Tipo) Para manejar estados de un objeto de manera segura y evitando estados inválidos.
 4. Clean Code y Sintaxis  
 5. Early Returns con el Operador ?  
 6. Extrae validaciones a funciones separadas, patron salvaguarda.
 7. Iteradores sobre Bucles 

---

## Test de integracion con Test containers
- Test de integración con test container que tenga todo lo necesario para la prueba de la integridad de la aplicacion con patrón optimizado de TestContainers para Rust** que minimice el uso de recursos computacionales. Investiga sobre Patrón Single Instance + Resource Pooling
## **🎯 Patrón Single Instance + Resource Pooling

---

## Correcciones de codigo
- Corrige todos los errores en el codigo completo del worspace incluide server y pasa todos los tests en verde.
- Los tests deber probar funcionalidad real de la aplicacion, funcionladad productiva, no solo la estructura de datos, o simulacion de la misma y siempre deben pasar en verde al finalizar la historia de usuario.
- Test de integración con test container que tenga todo lo necesario para la prueba de la integridad de la aplicacion con patrón optimizado de TestContainers para Rust** que minimice el uso de recursos computacionales. Investiga sobre Patrón Single Instance + Resource Pooling.
- No romper con el enfoque arquitectura DDD y el uso de buenas practicas de desarrollo.
- No procrastines y siempre haz la implementacion completa de los tests con sus diferentes variantes y casos de prueba.
- Intenta no dejar warnings en la compilacion de rust en el codigo.
- Utiliza siempre las ultimas versiones de las dependencias y librerias crates estables.
- Si te encuentras con problemas, busca soluciones en la comunidad de desarrollo o en la documentacion oficial, revisando el codigo directamente del crate a usar, con perplexity o context7.
- Muy importante: Implementa todo el codigo con foco a despliegue en produccion, codigo valido y seguro, no quiero imlementaciones inMemory en codigo productivo, no simulaciones ni funciones con respuesta inmediata y falseadas. El codigo debe ser production ready.
- No se toca las interfaces y ports a la ligera porque eso implica que se rompa el codigo de las implementaciones de todos los adaptadores. Cualquier cambio debe ser bien pensado y valorado antes de realizarlo.
- Cuando crees funcionalidad nueva o actualizada aplica estas buenas practicas:
 - The Builder Pattern (El Constructor) Para crear objetos complejos con muchas configuraciones opcionales, se usa el Builder.
 - The Newtype Pattern (Tipado Fuerte) Para crear tipos personalizados que encapsulan un tipo existente y proporcionan una interfaz más específica y segura.
 - Type State Pattern (Estado en el Tipo) Para manejar estados de un objeto de manera segura y evitando estados inválidos.
 - Clean Code y Sintaxis  
 - Early Returns con el Operador ?  
 - Extrae validaciones a funciones separadas, patron salvaguarda.
 - Iteradores sobre Bucles
 - No uses directamente las entidades de dominio para exponer el api rest, usa mapeo a DTOs (Data Transfer Objects) 


---

## Creacion de epicas segun especificaciones WEB.

Crea epicas y sus historias de usuario bien definidas segun especificaciones que te incluyo, con enfoque en:
- TDD de componentes web con testing library y mocks de servicios y siempre deben pasar en verde al finalizar la historia de usuario..
- Especificar que se haran pruebas de desarrollo en debug con mcp chrome devtools.
- Utiliza siempre las ultimas versiones de las dependencias y librerias crates estables.
- Si te encuentras con problemas, busca soluciones en la comunidad de desarrollo o en la documentacion oficial, revisando el codigo directamente del modulo de nodejs, con perplexity o context7.
- Con foco en el mejor diseño UX y Responsive Design.
- Respetar la paleta de colores y estilos del proyecto.
- Utilizar estilos responsivos y adaptativos para diferentes dispositivos y pantallas.


---

# Test de integracion en Axum

El equivalente a "levantar el servidor" (@SpringBootTest)
En Spring Boot, usas una anotación y el framework mágicamente levanta un Tomcat embebido en un puerto aleatorio antes de correr los tests.

En Rust, el enfoque es explícito. Tú controlas cómo y cuándo se inicia tu aplicación. Como Axum se ejecuta sobre el runtime asíncrono de Tokio, usas las herramientas de Tokio para esto.

El patrón estándar en Rust:
Modulariza tu main: Tu función main.rs no debe contener toda la lógica de inicio. Debes tener una función (por ejemplo, en lib.rs o un módulo startup.rs) que devuelva tu axum::Router.

Usa tokio::spawn en los tests: En tu función de test (marcada con #[tokio::test]), inicias tu servidor Axum en una tarea en segundo plano.

Usa el puerto 0: Configura el servidor de pruebas para escuchar en el puerto 0. Esto le dice al sistema operativo que asigne cualquier puerto TCP disponible. Luego, recuperas ese puerto para que tu cliente de pruebas sepa dónde llamar.

Ventaja de este enfoque: Es extremadamente rápido. Levantar un servidor Axum para un test toma milisegundos, a diferencia de los segundos que puede tomar Spring Boot.

Opción B: El enfoque "Rest Assured" (Librerías especializadas)
Si buscas esa experiencia fluida y específica para testing, existen librerías en el ecosistema de Rust.

La más recomendable actualmente para Axum es axum-test.

axum-test: Es una librería diseñada específicamente para probar aplicaciones Axum. Se encarga de "levantar" el servidor por ti (incluso puede hacerlo sin usar un puerto TCP real, inyectando las peticiones directamente, lo que es aún más rápido) y te da métodos cómodos para afirmar respuestas.

Ejemplo:
```rust
use axum_testing_example::app;
use axum_test::TestServer;
use serde_json::json;

#[tokio::test]
async fn test_health_check_fluent() -> anyhow::Result<()> {
    // 1. Configuración: axum-test se encarga de levantar la app
    let app_router = app();
    let server = TestServer::new(app_router)?;

    // 2. Ejecución y 3. Aserciones (Estilo fluido)
    let response = server
        .get("/health") // No necesitas la URL completa ni el puerto
        .await;

    // Aserciones estilo "Rest Assured"
    response.assert_status_ok();
    response.assert_json(&json!({
        "status": "ok"
    }));
    
    // También puedes verificar headers, texto plano, etc.
    // response.assert_header("content-type", "application/json");

    Ok(())
}
```


----

## Desarrollo web
Quiero probar toda la funcionalidad de los casos de uso principaples en la apliacion web para poder detectar errores en el render, errores logicos, errores de comunicacion con backend, errores de diseño, etc.
Desarrolla un flujo de trabajo completo integrando MCP Chrome DevTools para performance profiling, MCP Playwright para testing automatizado, y Perplexity para research de tecnologías.
"Analiza los requisitos web modernos para [tipo de aplicación] incluyendo:
- Patrones de UX/UI 2025
- APIs y servicios recomendados
- Optimizaciones de rendimiento específicas
- Consideraciones de accesibilidad"
- TDD de componentes web con testing library y mocks de servicios y siempre deben pasar en verde al finalizar la historia de usuario.
- Si te encuentras con problemas, busca soluciones en la comunidad de desarrollo o en la documentacion oficial, revisando el codigo directamente del modulo de nodejs, con perplexity o context7.
- Con foco en el mejor diseño UX y Responsive Design.
- Respetar la paleta de colores y estilos del proyecto.
- Utilizar estilos responsivos y adaptativos para diferentes dispositivos y pantallas.
- Mientras se va probando la aplicacion intentaremos extraer todos los flujos posibles a recrear con tests en playwright para probar de manera continua y automatizarlos con CI/CD.


HU-6.3 + HU-6.4 completadas (TDD, verde, production-ready)
Qué se ha implementado
Port nuevo (aditivo, no rompe interfaces):
crates/application/src/worker_command_sender.rs
Trait WorkerCommandSender con send_run_job(worker_id, job).
JobController (application):
crates/application/src/job_controller.rs
Método testeable run_once():
dequeue() de JobQueue
scheduling con SchedulingService
assign_to_job() en WorkerRegistry
send_run_job() vía port
persistencia en JobRepository
Adapter gRPC:
crates/grpc/src/worker_command_sender.rs
GrpcWorkerCommandSender convierte JobSpec → RunJobCommand (proto) y usa WorkerAgentServiceImpl::send_to_worker.
Wiring en producción:
crates/grpc/src/bin/server.rs
Loop background configurable por env:
HODEI_JOB_CONTROLLER_ENABLED=1 (default)
HODEI_JOB_CONTROLLER_INTERVAL_MS=500 (default)
Tests (todo en verde)
Se añadieron tests unitarios y de integración:
Unit: cargo test -p hodei-jobs-application job_controller::tests::run_once_assigns_and_dispatches
Integration gRPC: cargo test -p hodei-jobs-grpc --test job_controller_integration
Y se verificó:
cargo test --workspace ✅
Documentación
docs/epics/EPIC-6-KubernetesJobs-ControlLoop.md actualizado con checks visibles para HU-6.3+HU-6.4.
Estado del plan
HU-6.3+6.4: ✅ completadas
Siguientes pendientes:
HU-6.2 (OTP persistente + hardening dev-mode en prod)
HU-6.6 (Provisioning real sin stub)
HU-6.7 (JobTemplate/JobRun fase aditiva)
Si confirmas, el siguiente paso natural (y necesario para “production ready real”) es HU-6.2: sacar OTP de in-memory y eliminar aceptación de tokens dev-* en producción (manteniendo dev/testing bien delimitado).