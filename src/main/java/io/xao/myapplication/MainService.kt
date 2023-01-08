package io.xao.myapplication

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import kotlin.concurrent.thread

class NativeRunnable (context: Context): Runnable {
    val ctx:Context=context;

    public override fun run() {
        println("${Thread.currentThread()} has run.")
        this.onServiceStart()
    }

    public fun callbackGetFilesDir() : String{
        return this.ctx.filesDir.absolutePath;
    }

    public fun callbackFromNative(str:String){
        println("callback from native with string = $str")

    }

    public external fun onServiceStart()
}


class MainService : Service() {
    private val ChannelID = "ForegroundService Kotlin"
    companion object {
        fun startService(context: Context, message: String) {
            val startIntent = Intent(context, MainService::class.java)
            startIntent.putExtra("inputExtra", message)
            ContextCompat.startForegroundService(context, startIntent)
        }
        fun stopService(context: Context) {
            val stopIntent = Intent(context, MainService::class.java)
            context.stopService(stopIntent)
        }
    }




    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {

        //do heavy work on a background thread
        val input = intent?.getStringExtra("inputExtra")
        createNotificationChannel()
        val notificationIntent = Intent(this, MainActivity::class.java)
        val pendingIntent = PendingIntent.getActivity(
            this,
            0, notificationIntent, 0
        )
        val notification = NotificationCompat.Builder(this, ChannelID)
            .setContentTitle("Foreground Service Kotlin Example")
            .setContentText(input)
          //  .setSmallIcon(R.drawable.ic_notification)
            .setContentIntent(pendingIntent)
            .build()
        startForeground(1, notification)
        //stopSelf();

        val threadWithRunnable = Thread(NativeRunnable( this ))
        threadWithRunnable.start()
/*
        thread(start = true) {
            println("${Thread.currentThread()} has run.")
            MainBoot()
         }
*/
        return START_NOT_STICKY
    }
    override fun onBind(intent: Intent): IBinder? {
        return null
    }
    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val serviceChannel = NotificationChannel(ChannelID, "Foreground Service Channel",
                NotificationManager.IMPORTANCE_DEFAULT)
            val manager = getSystemService(NotificationManager::class.java)
            manager!!.createNotificationChannel(serviceChannel)
        }
    }
}