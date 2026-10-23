package org.mtrx.app.push

import android.content.Context
import android.content.pm.PackageManager
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONException
import org.json.JSONObject
import org.mtrx.app.model.Distributors
import org.mtrx.app.model.FALLBACK_GATEWAY
import org.mtrx.app.model.GatewayDiscovery
import org.mtrx.app.model.originGateway
import org.unifiedpush.android.connector.UnifiedPush
import org.unifiedpush.android.connector.data.ResolvedDistributor

/** The UnifiedPush connector (3.x), with its default instance. */
class UnifiedPushDistributors(private val context: Context) : Distributors {
    override fun installed(): List<String> = UnifiedPush.getDistributors(context)

    override fun saved(): String? = UnifiedPush.getSavedDistributor(context)

    override fun default(): String? =
        (UnifiedPush.resolveDefaultDistributor(context) as? ResolvedDistributor.Found)?.packageName

    override fun save(packageName: String) = UnifiedPush.saveDistributor(context, packageName)

    override fun register() = UnifiedPush.register(context)

    override fun unregister() = UnifiedPush.unregister(context)

    override fun label(packageName: String): String = try {
        val packages = context.packageManager
        packages.getApplicationLabel(packages.getApplicationInfo(packageName, 0)).toString()
    } catch (e: PackageManager.NameNotFoundException) {
        packageName
    }
}

/**
 * UnifiedPush's gateway discovery: the push server is the gateway if a GET
 * on its notify URL answers `{"unifiedpush":{"gateway":"matrix"}}`;
 * otherwise UnifiedPush's own gateway. Unreachable: assume it is one.
 */
object HttpGatewayDiscovery : GatewayDiscovery {
    override suspend fun gatewayFor(endpoint: String): String {
        val own = originGateway(endpoint) ?: return FALLBACK_GATEWAY
        return withContext(Dispatchers.IO) {
            val connection = try {
                URL(own).openConnection() as HttpURLConnection
            } catch (e: IOException) {
                return@withContext own
            }
            try {
                connection.connectTimeout = 5_000
                connection.readTimeout = 5_000
                val answer = if (connection.responseCode == HttpURLConnection.HTTP_OK) {
                    connection.inputStream.use { it.readBytes().toString(Charsets.UTF_8) }
                } else {
                    ""
                }
                if (isMatrixGateway(answer)) own else FALLBACK_GATEWAY
            } catch (e: IOException) {
                own
            } finally {
                connection.disconnect()
            }
        }
    }

    private fun isMatrixGateway(answer: String): Boolean = try {
        JSONObject(answer).optJSONObject("unifiedpush")?.optString("gateway") == "matrix"
    } catch (e: JSONException) {
        false
    }
}
